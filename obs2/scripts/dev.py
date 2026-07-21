#!/usr/bin/env python3

# Dev helper: runs the SvelteKit Vite dev server (live UI reload) and relinks the
# core on Rust changes, hot-reloading it via the production auto-update path -- stage
# into `.ge_update_staged/` and POST `/api/v1/updates/apply` (dev builds auto-apply).

from __future__ import annotations

import os
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from collections.abc import Mapping, Sequence
from pathlib import Path
from types import FrameType
from typing import Callable, NoReturn, Optional

IS_LINUX = sys.platform.startswith("linux")

ROOT = Path(__file__).resolve().parents[2]
BUILD_DIR = ROOT / "obs2" / "build"
PLUGIN_BUILD_DIR = ROOT / "obs2" / "build-flatpak" if IS_LINUX else BUILD_DIR
RUST_SRC = ROOT / "obs2" / "rust" / "src"
RUST_MANIFEST = ROOT / "obs2" / "rust" / "Cargo.toml"
PLUGIN_NAME = "the_golden_eye"
API_BASE = "http://127.0.0.1:31337"
ZELLIJ_LAYOUT = Path(__file__).with_name("dev.kdl")
DEV_READY_FILE_ENV = "GE_DEV_READY_FILE"
PLUGIN_LOG_PREFIX = b"[the_golden_eye]"
ANSI_RESET = b"\x1b[0m"


def plugin_log_color_enabled() -> bool:
    return (
        sys.stdout.isatty()
        and "NO_COLOR" not in os.environ
        and os.environ.get("TERM") != "dumb"
    )


def colorize_plugin_log_line(line: bytes, *, enabled: bool) -> bytes:
    marker = line.find(PLUGIN_LOG_PREFIX)
    if not enabled or marker < 0:
        return line

    plugin_log = line[marker:]
    if b"ERROR" in plugin_log:
        color = b"\x1b[1;91m"
    elif b"WARN" in plugin_log:
        color = b"\x1b[1;93m"
    elif b"DEBUG" in plugin_log:
        color = b"\x1b[36m"
    elif b"TRACE" in plugin_log:
        color = b"\x1b[2;36m"
    else:
        color = b"\x1b[33m"

    ending = b"\n" if line.endswith(b"\n") else b""
    content = line[: -len(ending)] if ending else line
    return color + content + ANSI_RESET + ending


def obs_plugin_paths() -> tuple[Path, Path]:
    if IS_LINUX:
        arch_dir = "64bit" if sys.maxsize > 2**32 else "32bit"
        return (
            PLUGIN_BUILD_DIR / "%module%" / "bin" / arch_dir,
            PLUGIN_BUILD_DIR / "%module%" / "data",
        )

    return PLUGIN_BUILD_DIR, PLUGIN_BUILD_DIR


def core_runtime_dir() -> Path:
    """Directory the built core library actually lives in -- the real
    on-disk path, unlike obs_plugin_paths()'s `%module%` placeholder (which
    OBS itself substitutes when scanning, not something to resolve here)."""
    if sys.platform == "darwin":
        return PLUGIN_BUILD_DIR / f"{PLUGIN_NAME}.plugin" / "Contents" / "MacOS"

    if IS_LINUX:
        arch_dir = "64bit" if sys.maxsize > 2**32 else "32bit"
        return PLUGIN_BUILD_DIR / PLUGIN_NAME / "bin" / arch_dir

    return PLUGIN_BUILD_DIR


def find_core_library(runtime_dir: Path) -> Path:
    # Globs rather than hardcoding a prefix/suffix (libgolden_core.dylib vs
    # libgolden_core.so vs golden_core.dll) -- a third place to encode that
    # convention isn't worth it when CMake and Rust already both know it.
    candidates = sorted(
        path for path in runtime_dir.glob("*golden_core*") if path.is_file()
    )
    if not candidates:
        raise FileNotFoundError(
            f"could not find the built core library under {runtime_dir}"
        )
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
            print(
                "[dev] reload deferred: plugin is busy monitoring/recording; it'll retry on its own",
                flush=True,
            )
        elif error.code == 404:
            print(
                "[dev] reload not applied: plugin reports nothing staged (unexpected)",
                file=sys.stderr,
            )
        else:
            print(f"[dev] reload request failed: HTTP {error.code}", file=sys.stderr)
    except urllib.error.URLError as error:
        print(
            f"[dev] could not reach the plugin to trigger a reload (is OBS running yet?): {error}",
            file=sys.stderr,
        )


class ProcessManager:
    def __init__(self) -> None:
        self.processes: list[subprocess.Popen[bytes]] = []
        self.output_threads: dict[subprocess.Popen[bytes], threading.Thread] = {}
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
        output_filter: Optional[Callable[[bytes], bytes]] = None,
    ) -> subprocess.Popen[bytes]:
        output = subprocess.PIPE if output_filter else None
        proc = subprocess.Popen(
            args,
            cwd=cwd,
            env=env,
            start_new_session=True,
            stdout=output,
            stderr=subprocess.STDOUT if output_filter else None,
        )
        with self.lock:
            self.processes.append(proc)
        if output_filter:
            output_thread = threading.Thread(
                target=forward_output,
                args=(proc, output_filter),
                daemon=True,
            )
            with self.lock:
                self.output_threads[proc] = output_thread
            output_thread.start()
        return proc

    def forget(self, proc: subprocess.Popen[bytes]) -> None:
        with self.lock:
            if proc in self.processes:
                self.processes.remove(proc)
            output_thread = self.output_threads.pop(proc, None)
        if output_thread:
            output_thread.join(timeout=1)

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


def forward_output(
    proc: subprocess.Popen[bytes], output_filter: Callable[[bytes], bytes]
) -> None:
    if proc.stdout is None:
        return

    try:
        for line in proc.stdout:
            sys.stdout.buffer.write(output_filter(line))
            sys.stdout.buffer.flush()
    except BrokenPipeError:
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


def dev_build_needs_configuration() -> bool:
    """Whether another workflow changed the shared build tree out of dev mode."""
    cache = BUILD_DIR / "CMakeCache.txt"
    try:
        entries = cache.read_text().splitlines()
    except OSError:
        return True

    return not {
        "BROWSER_DEV:BOOL=ON",
        "CMAKE_BUILD_TYPE:STRING=Debug",
    }.issubset(entries)


def rust_watch_loop(manager: ProcessManager, stop_event: threading.Event) -> None:
    last_seen = newest_rust_mtime()

    while not stop_event.wait(1):
        latest = newest_rust_mtime()
        if latest <= last_seen:
            continue

        last_seen = time.time()
        print("[dev] rust change detected; rebuilding core...", flush=True)
        if dev_build_needs_configuration():
            print("[dev] restoring Debug/BROWSER_DEV=ON configuration", flush=True)
            manager.run(["just", "configure-dev"])
        if IS_LINUX:
            # Relinking happens inside the Flatpak SDK for linux
            proc = manager.start(["just", "_dev-relink"], cwd=ROOT)
        else:
            proc = manager.start(
                ["cmake", "--build", ".", "--target", "golden_core"], cwd=BUILD_DIR
            )
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
            print(
                f"[dev] core rebuilt but failed to stage it for reload: {error}",
                file=sys.stderr,
            )
            continue
        trigger_reload_apply()


def install_signal_handlers(
    manager: ProcessManager, stop_event: threading.Event
) -> None:
    def handle_signal(signum: int, _frame: Optional[FrameType]) -> NoReturn:
        stop_event.set()
        manager.stop_all()
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)
    if hasattr(signal, "SIGHUP"):
        signal.signal(signal.SIGHUP, handle_signal)


def dev_ready_file() -> Path:
    ready_file = os.environ.get(DEV_READY_FILE_ENV)
    if not ready_file:
        raise RuntimeError("OBS pane needs a backend readiness file")
    return Path(ready_file)


def set_backend_status(status: str) -> None:
    try:
        dev_ready_file().write_text(status)
    except OSError as error:
        print(f"[dev] could not update backend status: {error}", file=sys.stderr)


def wait_for_backend() -> bool:
    ready_file = dev_ready_file()
    print("[dev] waiting for the backend's initial build...", flush=True)
    while True:
        try:
            status = ready_file.read_text()
        except FileNotFoundError:
            time.sleep(0.1)
            continue

        if status == "ready":
            return True
        if status == "failed":
            print("[dev] backend's initial build failed; OBS will not start", file=sys.stderr)
            return False
        time.sleep(0.1)


def run_dev(*, frontend: bool, backend: bool, obs: bool) -> int:
    manager = ProcessManager()
    stop_event = threading.Event()
    backend_failed = False
    install_signal_handlers(manager, stop_event)

    try:
        if frontend:
            vite = manager.start(["npm", "run", "dev"], cwd=ROOT / "obs2" / "browser")
            if not backend and not obs:
                vite_status = vite.wait()
                manager.forget(vite)
                return 128 - vite_status if vite_status < 0 else vite_status

        if obs and not backend and not wait_for_backend():
            return 1

        if backend:
            if IS_LINUX:
                # Linux needs to build in the flatpak env
                manager.run(["just", "_dev-build"])
            else:
                manager.run(["just", "configure-dev"])
                manager.run(["cmake", "--build", "obs2/build"])

            if not obs:
                set_backend_status("ready")

            watcher = threading.Thread(
                target=rust_watch_loop, args=(manager, stop_event), daemon=True
            )
            watcher.start()

        if obs:
            if IS_LINUX:
                obs_process = manager.start(
                    ["just", "_run-obs-flatpak"],
                    cwd=ROOT,
                    output_filter=lambda line: colorize_plugin_log_line(
                        line, enabled=plugin_log_color_enabled()
                    ),
                )
            else:
                plugin_path, data_path = obs_plugin_paths()
                env = os.environ.copy()
                env["OBS_PLUGINS_PATH"] = str(plugin_path)
                env["OBS_PLUGINS_DATA_PATH"] = str(data_path)
                obs_process = manager.start(
                    ["obs"],
                    env=env,
                    output_filter=lambda line: colorize_plugin_log_line(
                        line, enabled=plugin_log_color_enabled()
                    ),
                )
            obs_status = obs_process.wait()
            manager.forget(obs_process)

            stop_event.set()
            manager.stop_all()
            if backend:
                watcher.join(timeout=2)
            if frontend:
                manager.forget(vite)
            exit_status = 128 - obs_status if obs_status < 0 else obs_status
            if exit_status == 0:
                close_zellij_tab()
            else:
                print(
                    f"[dev] OBS exited with status {exit_status}; leaving this tab open",
                    file=sys.stderr,
                    flush=True,
                )
            return exit_status

        if backend:
            while not stop_event.wait(1):
                pass
            return 0

        raise ValueError("at least one dev process must be selected")
    except subprocess.CalledProcessError as error:
        print(
            f"[dev] command failed with status {error.returncode}: {' '.join(error.cmd)}",
            file=sys.stderr,
            flush=True,
        )
        if backend and not obs:
            backend_failed = True
            set_backend_status("failed")
        return 128 - error.returncode if error.returncode < 0 else error.returncode
    finally:
        stop_event.set()
        manager.stop_all()
        if backend and not obs and not backend_failed:
            try:
                dev_ready_file().unlink(missing_ok=True)
            except OSError:
                pass


def zellij_available() -> bool:
    return shutil.which("zellij") is not None


def inside_zellij() -> bool:
    return bool(os.environ.get("ZELLIJ"))


def close_zellij_tab() -> None:
    if inside_zellij() and zellij_available():
        subprocess.run(["zellij", "action", "close-tab"], check=False)


def main() -> int:
    if os.name != "posix":
        print("just dev requires POSIX process groups for cleanup.", file=sys.stderr)
        return 1

    pane = sys.argv[1:]
    if pane == ["--pane", "frontend"]:
        return run_dev(frontend=True, backend=False, obs=False)
    if pane == ["--pane", "backend"]:
        return run_dev(frontend=False, backend=True, obs=False)
    if pane == ["--pane", "obs"]:
        return run_dev(frontend=False, backend=False, obs=True)
    if pane:
        print(f"unknown arguments: {' '.join(pane)}", file=sys.stderr)
        return 2

    if zellij_available():
        ready_file = Path(tempfile.gettempdir()) / f"the-golden-eye-dev-{os.getpid()}.ready"
        ready_file.unlink(missing_ok=True)
        env = os.environ.copy()
        env[DEV_READY_FILE_ENV] = str(ready_file)
        return subprocess.run(["zellij", "--layout", str(ZELLIJ_LAYOUT)], env=env).returncode

    print(
        "\n"
        "============================================================\n"
        "[dev] Zellij is not installed; using a single terminal.\n"
        "[dev] Install zellij for a much better dev experience with\n"
        "      separate frontend, backend, and OBS output panes.\n"
        "============================================================\n",
        flush=True,
    )
    time.sleep(2.5)
    return run_dev(frontend=True, backend=True, obs=True)


if __name__ == "__main__":
    raise SystemExit(main())
