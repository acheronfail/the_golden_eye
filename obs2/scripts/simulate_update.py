#!/usr/bin/env python3

# Builds the source tree as a fake "newer" release (bumped GE_PLUGIN_VERSION) and
# serves it plus checksums.txt from a local HTTP server mimicking GitHub's releases API,
# to smoke-test auto-update end to end. Usage: `just simulate-update` (see README/repo).

from __future__ import annotations

import argparse
import hashlib
import http.server
import json
import os
import platform
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UPDATER_VERSION_FILE = ROOT / "obs2" / "updater-version.txt"
DIST_DIRS = [
    ROOT / "obs2" / "build" / "dist",
    ROOT / "obs2" / "build-flatpak" / "dist"
]
SERVER_PORT = 8990
RELEASE_URL = "https://github.com/acheronfail/the_golden_eye/releases/tag/simulated"


def git(*args: str) -> str:
    result = subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=False)
    return result.stdout.strip()


def bumped_version() -> str:
    base_tag = git("describe", "--tags", "--abbrev=0", "--match", "v[0-9]*.[0-9]*.[0-9]*")
    base = base_tag[1:] if base_tag.startswith("v") else base_tag
    major, minor, patch = (int(part) for part in (base or "0.0.0").split(".")[:3])
    return f"{major}.{minor}.{patch + 1}"


def positive_updater_version(value: str, source: str) -> int:
    if not value.isdigit() or int(value) < 1:
        raise SystemExit(f"{source} must be a positive integer, got {value!r}")
    return int(value)


def checked_in_updater_version() -> int:
    try:
        value = UPDATER_VERSION_FILE.read_text().strip()
    except OSError as error:
        raise SystemExit(f"cannot read {UPDATER_VERSION_FILE}: {error}") from error
    return positive_updater_version(value, str(UPDATER_VERSION_FILE))


def resolve_updater_version(command_line_value: str | None) -> int:
    if command_line_value is not None:
        return positive_updater_version(command_line_value, "--updater-version")
    if value := os.environ.get("GE_UPDATER_VERSION"):
        return positive_updater_version(value, "GE_UPDATER_VERSION")
    return checked_in_updater_version()


def package_platform() -> str:
    if sys.platform == "darwin":
        return "macos"
    if sys.platform == "win32":
        return "windows"
    if sys.platform.startswith("linux"):
        return "linux"
    raise SystemExit(f"unsupported package platform: {sys.platform}")


def package_arch() -> str:
    machine = platform.machine().lower()
    if machine in {"amd64", "x86_64"}:
        return "x86_64"
    if machine in {"aarch64", "arm64"}:
        return "arm64"
    raise SystemExit(f"unsupported package arch: {platform.machine()}")


def build_package(version: str, updater_version: int) -> Path:
    expected_name = (
        f"the_golden_eye-u{updater_version}-v{version}-{package_platform()}-{package_arch()}.zip"
    )
    print(
        f"[simulate-update] building v{version} with updater version u{updater_version}...",
        flush=True,
    )
    env = {
        **os.environ,
        "GE_PLUGIN_VERSION": version,
        "GE_UPDATER_VERSION": str(updater_version),
    }
    subprocess.run(["just", "make-package"], cwd=ROOT, env=env, check=True)
    matches = [directory / expected_name for directory in DIST_DIRS if (directory / expected_name).is_file()]
    if len(matches) != 1:
        print(
            f"[simulate-update] expected exactly one {expected_name} under {DIST_DIRS}, found {matches}",
            file=sys.stderr,
        )
        raise SystemExit(1)
    return matches[0]


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(64 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def make_handler(
    version: str,
    zip_path: Path,
    checksums_text: bytes,
    legacy_asset_name: str | None,
) -> type[http.server.BaseHTTPRequestHandler]:
    zip_bytes = zip_path.read_bytes()
    base_url = f"http://127.0.0.1:{SERVER_PORT}"
    package_assets = [
        {"name": zip_path.name, "browser_download_url": f"{base_url}/{zip_path.name}"}
    ]
    if legacy_asset_name:
        package_assets.append(
            {"name": legacy_asset_name, "browser_download_url": f"{base_url}/{legacy_asset_name}"}
        )
    latest_json = json.dumps(
        {
            "tag_name": f"v{version}",
            "html_url": RELEASE_URL,
            "assets": [
                *package_assets,
                {"name": "checksums.txt", "browser_download_url": f"{base_url}/checksums.txt"},
            ],
        }
    ).encode()

    class Handler(http.server.BaseHTTPRequestHandler):
        def _respond(self, body: bytes, content_type: str) -> None:
            self.send_response(200)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self) -> None:  # noqa: N802 (BaseHTTPRequestHandler's own naming)
            if self.path == "/latest":
                self._respond(latest_json, "application/json")
            elif self.path == f"/{zip_path.name}" or (
                legacy_asset_name and self.path == f"/{legacy_asset_name}"
            ):
                self._respond(zip_bytes, "application/zip")
            elif self.path == "/checksums.txt":
                self._respond(checksums_text, "text/plain")
            else:
                self.send_error(404)

        def log_message(self, format: str, *args: object) -> None:
            print(f"[simulate-update] {self.address_string()} {format % args}", flush=True)

    return Handler


def main() -> int:
    parser = argparse.ArgumentParser(description="Serve a local simulated plugin update.")
    parser.add_argument("--updater-version", metavar="N")
    parser.add_argument(
        "--legacy-asset-alias",
        action="store_true",
        help="also expose the temporary pre-u1 package name",
    )
    args = parser.parse_args()

    version = bumped_version()
    updater_version = resolve_updater_version(args.updater_version)
    installed_updater_version = checked_in_updater_version()
    zip_path = build_package(version, updater_version)
    checksum = sha256_of(zip_path)
    legacy_asset_name = (
        f"the_golden_eye-{version}-{package_platform()}-{package_arch()}.zip"
        if args.legacy_asset_alias
        else None
    )
    checksum_names = [zip_path.name, *([legacy_asset_name] if legacy_asset_name else [])]
    checksums_text = "".join(f"{checksum}  {name}\n" for name in checksum_names).encode()
    compatibility = "compatible" if updater_version == installed_updater_version else "manual install required"

    print(f"[simulate-update] serving fake release v{version} ({zip_path.name}, sha256 {checksum[:12]}...)")
    print(
        f"[simulate-update] target u{updater_version}; checked-in plugin support u{installed_updater_version}: "
        f"{compatibility}"
    )
    if legacy_asset_name:
        print(f"[simulate-update] also serving legacy alias {legacy_asset_name}")
    print(f"[simulate-update] run in another terminal:")
    print(f"[simulate-update]   GE_UPDATE_CHECK_URL=http://127.0.0.1:{SERVER_PORT}/latest just obs")
    if updater_version == installed_updater_version:
        print("[simulate-update] expect the update to download, stage, and apply")
    else:
        print("[simulate-update] expect a manual-install prompt and no package download request")
    print("[simulate-update] press Ctrl+C to stop", flush=True)

    handler = make_handler(version, zip_path, checksums_text, legacy_asset_name)
    server = http.server.HTTPServer(("127.0.0.1", SERVER_PORT), handler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
