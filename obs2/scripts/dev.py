#!/usr/bin/env python3

# The plugin is split into a thin shim (loaded by OBS) and a "core" library
# (the Rust logic + OpenCV), which the shim dlopen's. In dev mode this helper
# runs the SvelteKit Vite dev server (so editing the UI reloads live) and
# relinks the core whenever Rust sources change.
#
# Hot-reloading the rebuilt core into a running OBS session reuses the exact
# same mechanism as production auto-update, rather than a dev-only FIFO into
# the shim (which was removed when the shim was minimized -- see
# obs2/shim/reload.c): after a successful rebuild, this script copies the
# freshly built core library into the plugin's `.ge_update_staged/`
# directory (the same convention update_apply.rs uses) and POSTs to
# `/api/v1/updates/apply`. The Rust side treats a dev build as always
# "opted in" to auto-update (see update_apply.rs::auto_apply_when_safe), so
# if that POST is momentarily refused (e.g. a monitor session is active),
# the plugin's own background loop picks the staged rebuild up shortly after
# on its own -- no restart needed either way.

from __future__ import annotations

import os
import shutil
import signal
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from collections.abc import Mapping, Sequence
from pathlib import Path
from types import FrameType
from typing import NoReturn, Optional


ROOT = Path(__file__).resolve().parents[2]
BUILD_DIR = ROOT / "obs2" / "build"
RUST_SRC = ROOT / "obs2" / "rust" / "src"
RUST_MANIFEST = ROOT / "obs2" / "rust" / "Cargo.toml"
PLUGIN_NAME = "the_golden_eye"
API_BASE = "http://127.0.0.1:31337"


def obs_plugin_paths() -> tuple[Path, Path]:
    if sys.platform == "darwin":
        return BUILD_DIR, BUILD_DIR

    if sys.platform.startswith("linux"):
        arch_dir = "64bit" if sys.maxsize > 2**32 else "32bit"
        return BUILD_DIR / "%module%" / "bin" / arch_dir, BUILD_DIR / "%module%" / "data"

    return BUILD_DIR, BUILD_DIR


def core_runtime_dir() -> Path:
    """Directory the built core library actually lives in -- the real
    on-disk path, unlike obs_plugin_paths()'s `%module%` placeholder (which
    OBS itself substitutes when scanning, not something to resolve here)."""
    if sys.platform == "darwin":
        return BUILD_DIR / f"{PLUGIN_NAME}.plugin" / "Contents" / "MacOS"

    if sys.platform.startswith("linux"):
        arch_dir = "64bit" if sys.maxsize > 2**32 else "32bit"
        return BUILD_DIR / PLUGIN_NAME / "bin" / arch_dir

    return BUILD_DIR


def find_core_library(runtime_dir: Path) -> Path:
    # Globs rather than hardcoding a prefix/suffix (libgolden_core.dylib vs
    # libgolden_core.so vs golden_core.dll) -- a third place to encode that
    # convention isn't worth it when CMake and Rust already both know it.
    candidates = sorted(path for path in runtime_dir.glob("*golden_core*") if path.is_file())
    if not candidates:
        raise FileNotFoundError(f"could not find the built core library under {runtime_dir}")
    if len(candidates) > 1:
        print(
            f"[dev] warning: multiple core library files found under {runtime_dir}: {candidates}; using {candidates[0]}",
            file=sys.stderr,
        )
    return candidates[0]


def stage_dev_reload() -> None:
    runtime_dir = core_runtime_dir()
    core_lib = find_core_library(runtime_dir)
    staged_dir = runtime_dir / ".ge_update_staged"
    staged_dir.mkdir(exist_ok=True)
    shutil.copy2(core_lib, staged_dir / core_lib.name)


def trigger_reload_apply() -> None:
    request = urllib.request.Request(f"{API_BASE}/api/v1/updates/apply", method="POST")
    try:
        with urllib.request.urlopen(request, timeout=5) as response:
            print(f"[dev] reload applied (HTTP {response.status})", flush=True)
    except urllib.error.HTTPError as error:
        if error.code == 409:
            print("[dev] reload deferred: plugin is busy monitoring/recording; it'll retry on its own", flush=True)
        elif error.code == 404:
            print("[dev] reload not applied: plugin reports nothing staged (unexpected)", file=sys.stderr)
        else:
            print(f"[dev] reload request failed: HTTP {error.code}", file=sys.stderr)
    except urllib.error.URLError as error:
        print(f"[dev] could not reach the plugin to trigger a reload (is OBS running yet?): {error}", file=sys.stderr)


class ProcessManager:
    def __init__(self) -> None:
        self.processes: list[subprocess.Popen[bytes]] = []
        self.lock = threading.Lock()
        self.stopping = False

    def run(
        self,
        args: Sequence[str],
        *,
        cwd: Optional[Path] = None,
        env: Optional[Mapping[str, str]] = None,
    ) -> None:
        proc = self.start(args, cwd=ROOT if cwd is None else cwd, env=env)
        result = proc.wait()
        self.forget(proc)
        if result != 0:
            raise subprocess.CalledProcessError(result, args)

    def start(
        self,
        args: Sequence[str],
        *,
        cwd: Optional[Path] = None,
        env: Optional[Mapping[str, str]] = None,
    ) -> subprocess.Popen[bytes]:
        proc = subprocess.Popen(args, cwd=cwd, env=env, start_new_session=True)
        with self.lock:
            self.processes.append(proc)
        return proc

    def forget(self, proc: subprocess.Popen[bytes]) -> None:
        with self.lock:
            if proc in self.processes:
                self.processes.remove(proc)

    def stop_all(self) -> None:
        with self.lock:
            if self.stopping:
                return
            self.stopping = True
            processes = list(self.processes)

        for proc in processes:
            terminate_process_group(proc, signal.SIGTERM)

        time.sleep(0.5)

        for proc in processes:
            if proc.poll() is None:
                terminate_process_group(proc, signal.SIGKILL)

        for proc in processes:
            try:
                proc.wait(timeout=1)
            except subprocess.TimeoutExpired:
                pass


def terminate_process_group(proc: subprocess.Popen[bytes], sig: signal.Signals) -> None:
    if proc.poll() is not None:
        return

    try:
        os.killpg(proc.pid, sig)
    except ProcessLookupError:
        pass
    except PermissionError:
        try:
            proc.send_signal(sig)
        except ProcessLookupError:
            pass


def newest_rust_mtime() -> float:
    newest = RUST_MANIFEST.stat().st_mtime
    for path in RUST_SRC.rglob("*"):
        try:
            if path.is_file():
                newest = max(newest, path.stat().st_mtime)
        except FileNotFoundError:
            pass
    return newest


def rust_watch_loop(manager: ProcessManager, stop_event: threading.Event) -> None:
    last_seen = newest_rust_mtime()

    while not stop_event.wait(1):
        latest = newest_rust_mtime()
        if latest <= last_seen:
            continue

        last_seen = time.time()
        print("[dev] rust change detected; rebuilding core...", flush=True)
        proc = manager.start(["cmake", "--build", ".", "--target", "golden_core"], cwd=BUILD_DIR)
        result = proc.wait()
        manager.forget(proc)

        if stop_event.is_set():
            return
        if result != 0:
            print("[dev] core build failed; fix and save again", flush=True)
            continue

        try:
            stage_dev_reload()
        except OSError as error:
            print(f"[dev] core rebuilt but failed to stage it for reload: {error}", file=sys.stderr)
            continue
        trigger_reload_apply()


def install_signal_handlers(manager: ProcessManager, stop_event: threading.Event) -> None:
    def handle_signal(signum: int, _frame: Optional[FrameType]) -> NoReturn:
        stop_event.set()
        manager.stop_all()
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)
    if hasattr(signal, "SIGHUP"):
        signal.signal(signal.SIGHUP, handle_signal)


def main() -> int:
    if os.name != "posix":
        print("just dev requires POSIX process groups for cleanup.", file=sys.stderr)
        return 1

    manager = ProcessManager()
    stop_event = threading.Event()
    install_signal_handlers(manager, stop_event)

    try:
        manager.run(["just", "configure-dev"])
        manager.run(["cmake", "--build", "obs2/build"])

        vite = manager.start(["npm", "run", "dev"], cwd=ROOT / "obs2" / "browser")

        watcher = threading.Thread(target=rust_watch_loop, args=(manager, stop_event), daemon=True)
        watcher.start()

        plugin_path, data_path = obs_plugin_paths()
        env = os.environ.copy()
        env["OBS_PLUGINS_PATH"] = str(plugin_path)
        env["OBS_PLUGINS_DATA_PATH"] = str(data_path)
        obs = manager.start(["obs"], env=env)
        obs_status = obs.wait()
        manager.forget(obs)

        stop_event.set()
        manager.stop_all()
        watcher.join(timeout=2)
        manager.forget(vite)
        return 128 - obs_status if obs_status < 0 else obs_status
    except subprocess.CalledProcessError as error:
        return 128 - error.returncode if error.returncode < 0 else error.returncode
    finally:
        stop_event.set()
        manager.stop_all()


if __name__ == "__main__":
    raise SystemExit(main())
