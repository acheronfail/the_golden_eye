#!/usr/bin/env python3

# Builds the source tree as a fake "newer" release (bumped GE_PLUGIN_VERSION) and
# serves it plus checksums.txt from a local HTTP server mimicking GitHub's releases API,
# to smoke-test auto-update end to end. Usage: `just simulate-update` (see README/repo).

from __future__ import annotations

import hashlib
import http.server
import json
from itertools import chain
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
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


def build_package(version: str) -> Path:
    print(f"[simulate-update] building a package as v{version}...", flush=True)
    env = {**os.environ, "GE_PLUGIN_VERSION": version}
    subprocess.run(["just", "make-package"], cwd=ROOT, env=env, check=True)
    zips = sorted(chain.from_iterable(dir.glob("*.zip") for dir in DIST_DIRS))
    if not zips:
        print(f"[simulate-update] no package zip found under {DIST_DIRS}", file=sys.stderr)
        raise SystemExit(1)
    if len(zips) > 1:
        print(
            f"[simulate-update] warning: multiple package zips found under {DIST_DIRS}: {zips}; using {zips[-1]}",
            file=sys.stderr,
        )
    return zips[-1]


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(64 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def make_handler(version: str, zip_path: Path, checksums_text: bytes) -> type[http.server.BaseHTTPRequestHandler]:
    zip_bytes = zip_path.read_bytes()
    base_url = f"http://127.0.0.1:{SERVER_PORT}"
    latest_json = json.dumps(
        {
            "tag_name": f"v{version}",
            "html_url": RELEASE_URL,
            "assets": [
                {"name": zip_path.name, "browser_download_url": f"{base_url}/{zip_path.name}"},
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
            elif self.path == f"/{zip_path.name}":
                self._respond(zip_bytes, "application/zip")
            elif self.path == "/checksums.txt":
                self._respond(checksums_text, "text/plain")
            else:
                self.send_error(404)

        def log_message(self, format: str, *args: object) -> None:
            print(f"[simulate-update] {self.address_string()} {format % args}", flush=True)

    return Handler


def main() -> int:
    version = bumped_version()
    zip_path = build_package(version)
    checksum = sha256_of(zip_path)
    checksums_text = f"{checksum}  {zip_path.name}\n".encode()

    print(f"[simulate-update] serving fake release v{version} ({zip_path.name}, sha256 {checksum[:12]}...)")
    print(f"[simulate-update] run in another terminal:")
    print(f"[simulate-update]   GE_UPDATE_CHECK_URL=http://127.0.0.1:{SERVER_PORT}/latest just obs")
    print("[simulate-update] then, in the plugin's options page, enable auto-update or click 'Apply update now'")
    print("[simulate-update] press Ctrl+C to stop", flush=True)

    handler = make_handler(version, zip_path, checksums_text)
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
