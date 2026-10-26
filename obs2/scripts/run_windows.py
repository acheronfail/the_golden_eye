#!/usr/bin/env python3
"""Launch a local build with the installed OBS version's discovery layout."""
import os
from pathlib import Path
import shutil
import subprocess


def obs_executable() -> str:
    candidates = [Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "obs-studio/bin/64bit/obs64.exe",
                  Path(os.environ.get("SCOOP", str(Path.home() / "scoop"))) / "apps/obs-studio/current/bin/64bit/obs64.exe"]
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)
    if executable := shutil.which("obs"):
        return executable
    raise RuntimeError("OBS was not found in Program Files, Scoop, or PATH")


def launch_environment(build: Path) -> dict[str, str]:
    candidates = [build, build / "Release", build / "Debug"]
    runtime = max((p for p in candidates if (p / "golden_core.dll").is_file()),
                  key=lambda p: (p / "golden_core.dll").stat().st_mtime)
    executable = obs_executable().replace("'", "''")
    major = int(subprocess.check_output(["powershell", "-NoProfile", "-Command",
        f"(Get-Item -LiteralPath '{executable}').VersionInfo.FileMajorPart"], text=True).strip())
    env = dict(os.environ)
    env["OBS_PLUGINS_DATA_PATH"] = str(build / "obs-run-data")
    if major >= 33:
        subprocess.run(["cmake", "--build", str(build), "--config", runtime.name if runtime != build else "Release",
                        "--target", "stage-plugin"], check=True)
        bundles = list((build / "package").glob("the_golden_eye-windows-*"))
        if len(bundles) != 1:
            raise RuntimeError(f"Expected one Windows bundle, found {bundles}")
        env["OBS_PLUGINS_PATH"] = str(bundles[0])
        env.setdefault("GE_CORE_LIB", str(runtime / "golden_core.dll"))
    else:
        env["OBS_PLUGINS_PATH"] = str(runtime)
    return env


if __name__ == "__main__":
    executable = obs_executable()
    build = Path(__file__).resolve().parents[1] / "build"
    raise SystemExit(subprocess.call([executable], cwd=str(Path(executable).parent), env=launch_environment(build)))
