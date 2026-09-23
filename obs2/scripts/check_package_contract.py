#!/usr/bin/env python3

from __future__ import annotations

import argparse
import os
import platform
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath


PLUGIN_NAME = "the_golden_eye"
CORE_ENTRY_POINTS = {
    "ge_core_load_v2",
    "ge_core_post_load",
    "ge_core_commit_update",
    "ge_core_unload",
}


def validate_core_exports(archive: zipfile.ZipFile, package: str) -> None:
    if package == "macos":
        core_path = f"{PLUGIN_NAME}.plugin/Contents/MacOS/libgolden_core.dylib"
        command = ["nm", "-gUj"]
    elif package == "windows":
        core_path = f"{PLUGIN_NAME}/bin/64bit/golden_core.dll"
        command = ["dumpbin", "/nologo", "/exports"]
    else:
        core_path = f"{PLUGIN_NAME}/bin/64bit/libgolden_core.so"
        command = ["nm", "-D", "--defined-only", "--format=posix"]

    with tempfile.TemporaryDirectory(prefix="ge-package-contract-") as temporary:
        core = Path(temporary) / PurePosixPath(core_path).name
        core.write_bytes(archive.read(core_path))
        result = subprocess.run([*command, str(core)], check=True, capture_output=True, text=True)

    exported = set()
    for line in result.stdout.splitlines():
        fields = line.split()
        if not fields:
            continue
        if package == "windows":
            if len(fields) < 4 or not fields[0].isdigit():
                continue
            symbol = fields[3]
        else:
            symbol = fields[0]
        exported.add(symbol.removeprefix("_") if package == "macos" else symbol)

    missing = sorted(CORE_ENTRY_POINTS - exported)
    if missing:
        raise SystemExit(f"packaged core is missing exported loader entry points: {missing!r}")


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


def required_paths(package: str) -> tuple[str, set[str]]:
    if package == "macos":
        root = f"{PLUGIN_NAME}.plugin"
        return root, {
            f"{root}/Contents/MacOS/{PLUGIN_NAME}",
            f"{root}/Contents/MacOS/libgolden_core.dylib",
            f"{root}/Contents/Resources/cv_templates/",
            f"{root}/Contents/Resources/locale/en-US.ini",
        }
    if package == "windows":
        root = PLUGIN_NAME
        return root, {
            f"{root}/bin/64bit/{PLUGIN_NAME}.dll",
            f"{root}/bin/64bit/golden_core.dll",
            f"{root}/data/cv_templates/",
            f"{root}/data/locale/",
            f"{root}/data/locale/en-US.ini",
        }
    root = PLUGIN_NAME
    return root, {
        f"{root}/bin/64bit/{PLUGIN_NAME}.so",
        f"{root}/bin/64bit/libgolden_core.so",
        f"{root}/data/cv_templates/",
        f"{root}/data/locale/",
        f"{root}/data/locale/en-US.ini",
    }


def validate_obs_run_data(build_dir: Path, archive: zipfile.ZipFile, names: set[str]) -> None:
    package_prefix = f"{PLUGIN_NAME}/data/"
    packaged_files = {
        name.removeprefix(package_prefix)
        for name in names
        if name.startswith(package_prefix) and not name.endswith("/")
    }
    run_root = build_dir / "obs-run-data" / PLUGIN_NAME
    if not run_root.is_dir():
        raise SystemExit(f"OBS run data root is missing: {run_root}")

    run_files = {
        path.relative_to(run_root).as_posix()
        for path in run_root.rglob("*")
        if path.is_file()
    }
    if run_files != packaged_files:
        missing = sorted(packaged_files - run_files)
        unexpected = sorted(run_files - packaged_files)
        raise SystemExit(
            f"OBS run data differs from packaged data; missing={missing!r}, "
            f"unexpected={unexpected!r}"
        )

    nested = run_root / PLUGIN_NAME
    if nested.exists():
        raise SystemExit(f"OBS run data must not contain a nested module directory: {nested}")

    for relative in sorted(packaged_files):
        packaged = archive.read(f"{package_prefix}{relative}")
        running = (run_root / relative).read_bytes()
        if running != packaged:
            raise SystemExit(f"OBS run data content differs from package: {relative}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate The Golden Eye package naming, layout, and core exports.")
    parser.add_argument("build_dir", help="CMake build directory containing dist/*.zip")
    args = parser.parse_args()

    version = os.environ.get("GE_PLUGIN_VERSION")
    if not version:
        raise SystemExit("GE_PLUGIN_VERSION is required")
    updater_version = os.environ.get("GE_UPDATER_VERSION")
    if not updater_version or not updater_version.isdigit():
        raise SystemExit("GE_UPDATER_VERSION must be a non-negative integer")

    package = package_platform()
    arch = package_arch()
    expected_zip_name = f"{PLUGIN_NAME}-u{updater_version}-v{version}-{package}-{arch}.zip"
    dist_dir = os.path.join(args.build_dir, "dist")
    zips = sorted(name for name in os.listdir(dist_dir) if name.startswith(f"{PLUGIN_NAME}-") and name.endswith(".zip"))
    if zips != [expected_zip_name]:
        raise SystemExit(f"expected exactly [{expected_zip_name!r}] in {dist_dir}, found {zips!r}")

    zip_path = os.path.join(dist_dir, expected_zip_name)
    expected_root, expected_paths = required_paths(package)
    with zipfile.ZipFile(zip_path) as archive:
        names = set(archive.namelist())
        if package != "macos":
            validate_obs_run_data(Path(args.build_dir), archive, names)

    roots = {PurePosixPath(name).parts[0] for name in names if PurePosixPath(name).parts}
    if roots != {expected_root}:
        raise SystemExit(f"{expected_zip_name} must contain only root {expected_root!r}, found {sorted(roots)!r}")

    version_entries = sorted(name for name in names if PurePosixPath(name).name == "VERSION")
    if version_entries:
        raise SystemExit(f"{expected_zip_name} must not ship VERSION files, found {version_entries!r}")

    missing = sorted(path for path in expected_paths if path not in names)
    if missing:
        raise SystemExit(f"{expected_zip_name} is missing required package paths: {missing!r}")

    with zipfile.ZipFile(zip_path) as archive:
        validate_core_exports(archive, package)

    print(f"Package contract OK: {expected_zip_name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
