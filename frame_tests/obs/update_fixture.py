#!/usr/bin/env python3
"""Serve update packages for the real OBS tests through the simulator's handler."""
import argparse
import http.server
import importlib.util
import json
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("simulate_update", ROOT / "obs2/scripts/simulate_update.py")
simulator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(simulator)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("plugin", type=Path)
    parser.add_argument("data", type=Path)
    parser.add_argument("--package", type=Path)
    parser.add_argument("--version")
    args = parser.parse_args()
    directory, plugin = args.directory, args.plugin
    directory.mkdir(parents=True, exist_ok=True)
    if args.package:
        if not args.version:
            parser.error("--package requires --version")
        checksum = f"{simulator.sha256_of(args.package)}  {args.package.name}\n".encode()
        packages = {"valid": simulator.make_handler(args.version, args.package, checksum)}
    else:
        mac = sys.platform == "darwin"
        core = "Contents/MacOS/libgolden_core.dylib" if mac else "bin/64bit/libgolden_core.so"
        data = "Contents/Resources" if mac else "data"
        name = f"the_golden_eye-u{simulator.checked_in_updater_version()}-v999.0.0-{simulator.package_platform()}-{simulator.package_arch()}.zip"
        bad_core = directory / ("bad.dylib" if mac else "bad.so")
        subprocess.run([
            "cc", "-dynamiclib" if mac else "-shared", "-fPIC",
            "-DGE_FIXTURE_GENERATION=99", "-DGE_FIXTURE_LOAD_FAILS=1",
            str(ROOT / "obs2/loader/tests/fixture.c"), "-o", str(bad_core),
        ], check=True)
        packages = {}
        for mode in ("valid", "rollback"):
            target = directory / mode / name
            target.parent.mkdir()
            with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED, compresslevel=1) as archive:
                archive.write(plugin / core if mode == "valid" else bad_core, f"{plugin.name}/{core}")
                for file in sorted(args.data.rglob("*")):
                    if file.is_file():
                        archive.write(file, f"{plugin.name}/{data}/{file.relative_to(args.data)}")
                archive.writestr(f"{plugin.name}/{data}/obs-update-test.txt", mode)
            checksum = f"{simulator.sha256_of(target)}  {name}\n".encode()
            packages[mode] = simulator.make_handler("999.0.0", target, checksum)
            if mode == "valid":
                packages["checksum"] = simulator.make_handler("999.0.0", target, f"{'0' * 64}  {name}\n".encode())

    class Server(http.server.HTTPServer):
        def finish_request(self, request, client_address):
            mode = (directory / "mode").read_text().strip()
            packages[mode](request, client_address, self)

    (directory / "mode").write_text("valid")
    with Server(("127.0.0.1", 0), packages["valid"]) as server:
        ready = directory / "ready.json"
        temporary = ready.with_suffix(".tmp")
        temporary.write_text(json.dumps({"url": f"http://127.0.0.1:{server.server_port}/latest"}))
        temporary.replace(ready)
        server.serve_forever()


if __name__ == "__main__":
    main()
