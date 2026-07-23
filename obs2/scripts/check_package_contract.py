#!/usr/bin/env python3

from __future__ import annotations

import argparse
import os
import platform
import sys
import zipfile
from pathlib import PurePosixPath


PLUGIN_NAME = "the_golden_eye"


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


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate The Golden Eye package artifact naming/layout.")
    parser.add_argument("build_dir", help="CMake build directory containing dist/*.zip")
    args = parser.parse_args()

    version = os.environ.get("GE_PLUGIN_VERSION")
    if not version:
        raise SystemExit("GE_PLUGIN_VERSION is required")
    updater_version = os.environ.get("GE_UPDATER_VERSION")
    if not updater_version or not updater_version.isdigit() or int(updater_version) < 1:
        raise SystemExit("GE_UPDATER_VERSION must be a positive integer")

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

    roots = {PurePosixPath(name).parts[0] for name in names if PurePosixPath(name).parts}
    if roots != {expected_root}:
        raise SystemExit(f"{expected_zip_name} must contain only root {expected_root!r}, found {sorted(roots)!r}")

    version_entries = sorted(name for name in names if PurePosixPath(name).name == "VERSION")
    if version_entries:
        raise SystemExit(f"{expected_zip_name} must not ship VERSION files, found {version_entries!r}")

    missing = sorted(path for path in expected_paths if path not in names)
    if missing:
        raise SystemExit(f"{expected_zip_name} is missing required package paths: {missing!r}")

    print(f"Package contract OK: {expected_zip_name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
