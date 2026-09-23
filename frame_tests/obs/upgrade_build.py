#!/usr/bin/env python3
"""Build two versions from one source tree and run the OBS upgrade test."""
import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("simulate_update", ROOT / "obs2/scripts/simulate_update.py")
simulator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(simulator)


def source_digest():
    files = subprocess.check_output(["git", "ls-files", "-co", "--exclude-standard", "-z", "--", "obs2", "justfile"], cwd=ROOT).split(b"\0")
    digest = hashlib.sha256()
    for name in sorted(set(filter(None, files))):
        file = ROOT / os.fsdecode(name)
        digest.update(name + b"\0")
        contents = file.read_bytes() if file.is_file() else b"missing"
        # Exporters write LF even when Git checks out these contracts with CRLF.
        if name in (b"obs2/browser/src/lib/generated/api.ts", b"obs2/browser/src/lib/generated/settings.ts"):
            contents = contents.replace(b"\r\n", b"\n")
        digest.update(contents)
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--software-renderer", action="store_true")
    parser.add_argument("--reuse-build", type=Path)
    parser.add_argument("--build-only", action="store_true")
    args = parser.parse_args()
    if sys.platform not in ("linux", "darwin", "win32"):
        parser.error("requires Linux, macOS, or Windows")
    if args.software_renderer and sys.platform != "linux":
        parser.error("--software-renderer is only supported on Linux")
    fingerprint = source_digest()
    identity = {"source": fingerprint, "platform": simulator.package_platform(), "arch": simulator.package_arch()}
    if args.reuse_build:
        directory = args.reuse_build.resolve()
        manifest = json.loads((directory / "builds.json").read_text())
        if manifest["identity"] != identity:
            parser.error("saved builds do not match the current source or platform")
        for build in manifest["builds"]:
            if simulator.sha256_of(directory / build["package"]) != build["sha256"]:
                parser.error("saved package checksum differs from the build manifest")
    else:
        base = ROOT / "obs2/build"
        base.mkdir(exist_ok=True)
        directory = Path(tempfile.mkdtemp(prefix="obs-upgrade-builds-", dir=base))
        manifest = {"identity": identity, "builds": []}
        print(f"Upgrade builds: {directory}", flush=True)
        for version in ("998.0.0", "998.0.1"):
            start = time.monotonic()
            package = simulator.build_package(version, simulator.checked_in_updater_version())
            target = directory / package.name
            shutil.copy2(package, target)
            manifest["builds"].append({"version": version, "package": target.name, "sha256": simulator.sha256_of(target), "buildSeconds": round(time.monotonic() - start, 1)})
        if source_digest() != fingerprint:
            raise RuntimeError("source files changed during the two builds; run again")
        (directory / "builds.json").write_text(json.dumps(manifest, indent=2))
    # Extract fresh copies so a prior test cannot change the next test's inputs.
    for index, build in enumerate(manifest["builds"]):
        destination = directory / f"version-{index}"
        if destination.exists():
            shutil.rmtree(destination)
        with zipfile.ZipFile(directory / build["package"]) as archive:
            archive.extractall(destination)
    print(f"Upgrade manifest: {directory / 'builds.json'}", flush=True)
    if not args.build_only:
        command = ["node", "--experimental-strip-types", "frame_tests/obs/upgrade.ts", str(directory)]
        if args.software_renderer:
            command.append("--software-renderer")
        return subprocess.call(command, cwd=ROOT)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
