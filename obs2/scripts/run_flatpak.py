#!/usr/bin/env python3
"""Launch the local universal bundle through both OBS discovery layouts."""
import argparse
import os
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("build", type=Path)
    parser.add_argument("--packaged", action="store_true")
    args = parser.parse_args()
    build = args.build.resolve()
    plugin, = (build / "package").glob("the_golden_eye-linux-*/the_golden_eye")
    launch = build / "obs-launch"
    link = launch / "data/obs-studio/plugins/the_golden_eye"
    link.parent.mkdir(parents=True, exist_ok=True)
    link.unlink(missing_ok=True)
    link.symlink_to(plugin, target_is_directory=True)
    core = plugin / "bin/64bit/libgolden_core.so" if args.packaged else build / "the_golden_eye/bin/64bit/libgolden_core.so"
    command = ["flatpak", "run", "--device=dri", f"--filesystem={build}", "--socket=session-bus",
               "--talk-name=org.freedesktop.secrets", "--talk-name=org.freedesktop.portal.Desktop",
               "--env=LD_LIBRARY_PATH=/app/lib",
               f"--env=GE_CORE_LIB={os.environ.get('GE_CORE_LIB', core)}",
               f"--env=OBS_PLUGINS_PATH={plugin / 'bin/64bit'}",
               f"--env=OBS_PLUGINS_DATA_PATH={build / 'obs-run-data'}",
               f"--env=OBS_LEGACY_PLUGINS_PATH={plugin / 'bin/64bit'}",
               f"--env=OBS_LEGACY_PLUGINS_DATA_PATH={build / 'obs-run-data'}"]
    for name in ["RUST_LOG", "GE_UPDATE_CHECK_URL"]:
        if name in os.environ:
            command.append(f"--env={name}={os.environ[name]}")
    command.extend(["--command=env", "com.obsproject.Studio", f"XDG_DATA_HOME={launch / 'data'}", "/app/bin/obs"])
    os.execvp(command[0], command)


if __name__ == "__main__":
    main()
