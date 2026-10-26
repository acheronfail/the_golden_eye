#!/usr/bin/env python3
"""Launch the local universal bundle through both OBS discovery layouts."""
import argparse
import os
from pathlib import Path
import platform


def link_directory(link: Path, target: Path):
    link.parent.mkdir(parents=True, exist_ok=True)
    if link.is_symlink():
        link.unlink()
    link.symlink_to(target, target_is_directory=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("build", type=Path)
    parser.add_argument("--packaged", action="store_true")
    args = parser.parse_args()
    build = args.build.resolve()
    arch = {"aarch64": "arm64", "amd64": "x86_64"}.get(platform.machine().lower(), platform.machine().lower())
    plugin = build / f"package/the_golden_eye-linux-{arch}/the_golden_eye"
    launch = build / "obs-launch"
    link_directory(launch / "data/obs-studio/plugins/the_golden_eye", plugin)
    data = plugin / "data" if args.packaged else build / "obs-run-data/the_golden_eye"
    link_directory(launch / "legacy-data/the_golden_eye", data)
    core = plugin / "bin/64bit/libgolden_core.so" if args.packaged else build / "the_golden_eye/bin/64bit/libgolden_core.so"
    command = ["flatpak", "run", "--device=dri", f"--filesystem={build}", "--socket=session-bus",
               "--talk-name=org.freedesktop.secrets", "--talk-name=org.freedesktop.portal.Desktop",
               "--env=LD_LIBRARY_PATH=/app/lib",
               f"--env=GE_CORE_LIB={os.environ.get('GE_CORE_LIB', core)}",
               f"--env=OBS_PLUGINS_PATH={plugin / 'bin/64bit'}",
               f"--env=OBS_PLUGINS_DATA_PATH={launch / 'legacy-data'}",
               f"--env=OBS_LEGACY_PLUGINS_PATH={plugin / 'bin/64bit'}",
               f"--env=OBS_LEGACY_PLUGINS_DATA_PATH={launch / 'legacy-data'}"]
    for name in ["RUST_LOG", "GE_UPDATE_CHECK_URL"]:
        if name in os.environ:
            command.append(f"--env={name}={os.environ[name]}")
    command.extend(["--command=env", "com.obsproject.Studio", f"XDG_DATA_HOME={launch / 'data'}", "/app/bin/obs"])
    os.execvp(command[0], command)


if __name__ == "__main__":
    main()
