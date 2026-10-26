#!/usr/bin/env python3
"""Launch a staged local build using the installed OBS discovery layout."""
import os
from pathlib import Path
import subprocess


OBS_EXECUTABLE = Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "obs-studio/bin/64bit/obs64.exe"


def runtime_directory(build: Path, configuration: str) -> Path:
    configured = build / configuration
    return configured if (configured / "golden_core.dll").is_file() else build


def launch_environment(build: Path, configuration: str) -> dict[str, str]:
    runtime = runtime_directory(build, configuration)
    executable = str(OBS_EXECUTABLE).replace("'", "''")
    major = int(subprocess.check_output(["powershell", "-NoProfile", "-Command",
        f"(Get-Item -LiteralPath '{executable}').VersionInfo.FileMajorPart"], text=True).strip())
    env = dict(os.environ)
    env["OBS_PLUGINS_DATA_PATH"] = str(build / "obs-run-data")
    if major >= 33:
        bundle, = (build / "package").glob("the_golden_eye-windows-*")
        env["OBS_PLUGINS_PATH"] = str(bundle)
        env.setdefault("GE_CORE_LIB", str(runtime / "golden_core.dll"))
    else:
        env["OBS_PLUGINS_PATH"] = str(runtime)
    return env


if __name__ == "__main__":
    build = Path(__file__).resolve().parents[1] / "build"
    raise SystemExit(subprocess.call([str(OBS_EXECUTABLE)], cwd=OBS_EXECUTABLE.parent,
                                    env=launch_environment(build, "Release")))
