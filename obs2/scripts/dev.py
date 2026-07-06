#!/usr/bin/env python3

from __future__ import annotations

import errno
import os
import signal
import stat
import subprocess
import sys
import threading
import time
from collections.abc import Mapping, Sequence
from pathlib import Path
from types import FrameType
from typing import NoReturn, Optional


ROOT = Path(__file__).resolve().parents[2]
BUILD_DIR = ROOT / "obs2" / "build"
RUST_SRC = ROOT / "obs2" / "rust" / "src"
RUST_MANIFEST = ROOT / "obs2" / "rust" / "Cargo.toml"
RELOAD_FIFO = Path(os.environ.get("TMPDIR", "/tmp")) / "ge_the_golden_eye.reload"


def obs_plugin_paths() -> tuple[Path, Path]:
    if sys.platform == "darwin":
        return (
            BUILD_DIR / "%module%.plugin" / "Contents" / "MacOS",
            BUILD_DIR / "%module%.plugin" / "Contents" / "Resources",
        )

    if sys.platform.startswith("linux"):
        arch_dir = "64bit" if sys.maxsize > 2**32 else "32bit"
        return BUILD_DIR / "%module%" / "bin" / arch_dir, BUILD_DIR / "%module%" / "data"

    return BUILD_DIR, BUILD_DIR


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


def ping_reload_fifo() -> None:
    try:
        mode = RELOAD_FIFO.stat().st_mode
    except FileNotFoundError:
        return

    if not stat.S_ISFIFO(mode):
        return

    try:
        fd = os.open(RELOAD_FIFO, os.O_WRONLY | os.O_NONBLOCK)
    except OSError as error:
        if error.errno != errno.ENXIO:
            print(f"[dev] could not ping reload FIFO: {error}", file=sys.stderr)
        return

    with os.fdopen(fd, "wb", closefd=True) as fifo:
        fifo.write(b"\n")


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
        if result == 0:
            ping_reload_fifo()
        else:
            print("[dev] core build failed; fix and save again", flush=True)


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

    os.environ["GE_RELOAD_FIFO"] = str(RELOAD_FIFO)

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
