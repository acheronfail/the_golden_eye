# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project is

"The Golden Eye" is an automation system that watches a GoldenEye 007 (N64) capture in OBS, detects level start/end screens, parses level info from the on-screen stats overlay, and drives OBS recording around runs. Discord/YouTube integration posts a "now streaming" notification when an OBS YouTube stream starts.

The repo holds **two implementations** living side by side, plus an unrelated ESP32 firmware:

- `obs/` — the original Node.js + OBS WebSocket implementation (driven by `just run`). Spawns a `llama.cpp` server for VLM-based text extraction and a pool of opencv4nodejs worker processes for template matching.
- `obs2/` — the rewrite as a native OBS plugin (driven by `just obs` / `just dev`). A thin C shim (`plugin.c`) is the module OBS loads; it `dlopen`s a separate "core" library (`core.c` + `obs_bridge.c`) that hosts a Rust static library (`obs2/rust/`) running an Axum HTTP server on port **31337** and serving a SvelteKit SPA. Template matching is done directly in Rust via the `opencv` crate. This is the "upcoming version."
- `esp32-input-monitor/` — independent PlatformIO firmware that sniffs N64 controller DATA lines and exposes state over WebSocket. Has its own `README.md` and `pio` build; not wired into the rest of the repo.

## Architecture (obs2 — the active implementation)

The runtime is a layered stack glued together by CMake:

1. **C shim** (`obs2/plugin.c`) — the library OBS actually loads. It contains no logic; `obs_module_load` `dlopen`s the core library (path baked in at build time as `GE_CORE_LIB_PATH`, overridable via the `GE_CORE_LIB` env var) with `RTLD_NOW` so link errors surface as a catchable, logged `dlopen` failure, then calls `ge_core_load()`. In dev builds (`GE_DEV`, set when `BROWSER_DEV=ON`) it dlopens a throwaway copy (the loader caches by path, so a fresh copy guarantees the rebuilt image loads) and arms the hot reload in `dev_reload.c`: a background thread blocks reading a FIFO (`GE_RELOAD_FIFO`), and on each ping reloads the core (`ge_core_unload` → `dlclose` → reopen → `ge_core_load`). The dev build pings the FIFO after each successful rebuild — no polling.
2. **C core** (`obs2/core.c`, `obs_bridge.c`) — the heavy library the shim hosts. `ge_core_load` calls `ge_rust_start()` and registers the frontend callback; on `OBS_FRONTEND_EVENT_STREAMING_STARTED` (for YouTube) it calls `ge_stream_notifier_start()`. `ge_core_unload` tears it all down (`ge_rust_stop` blocks until the tokio runtime is gone, so no Rust threads survive the `dlclose`). `obs_bridge.c` exposes helpers to Rust (e.g. `ge_obs_get_source_frame` renders an OBS source to a BGRA buffer). Built as a plain shared library in `build/core/` (a subdir so OBS's plugin scan ignores it); links the Rust staticlib, OpenCV, and the OBS libs.
3. **Rust staticlib** (`obs2/rust/`, crate `ge_rust`) — `lib.rs` owns a global `tokio::Runtime` inside a `Mutex<Option<ServerHandle>>`. FFI entry points (`ge_rust_start`, `ge_rust_stop`, `ge_stream_notifier_start/stop`) are `extern "C"` and spawn work onto the runtime without blocking the caller. `cv.rs` contains the level/time matcher; `stream_notifier.rs` posts Discord webhooks; `http/` is the Axum app.
4. **Axum HTTP server** — listens on `0.0.0.0:31337`. Routes: `/api/v1/record/{start,stop}`, `/api/v1/sources`, `/api/v1/screenshot`, `/oauth/callback`, and `/` (serves the embedded SPA via `include_str!(env!("BROWSER_BUNDLE"))`). Shared `AppState` holds the pending OAuth oneshot sender, the last Discord stream message (so stop can edit it), and `Config`.
5. **SvelteKit SPA** (`obs2/browser/`) — Svelte 5 + Tailwind v4 + Vite, built with `@sveltejs/adapter-static`. Output `build/index.html` is embedded into the Rust binary at compile time.

### Build coupling — important

The CMake build (`obs2/CMakeLists.txt`) wires these dependencies as a strict chain:

- `browser_build` runs `npm run build` in `obs2/browser/`, producing `obs2/browser/build/index.html`.
- `rust_build` depends on `browser_build`. `cargo build` runs with `BROWSER_BUNDLE` set to that path; the Rust crate embeds it via `include_str!`. `build.rs` also runs `cbindgen` and writes `obs2/ge_rust.h` (used by `plugin.c`).
- The plugin target depends on `rust_libs` (an `IMPORTED STATIC` library pointing at `target/{debug,release}/libge_rust.a`).

A failed frontend build stops the chain before cargo runs — don't try to bypass this.

**Dev mode** (`-DBROWSER_DEV=ON`, used by `just dev`):
- Skips the SPA build. Embeds a tiny redirect HTML pointing at `http://localhost:5173` (the Vite dev server).
- Enables the Rust `dev` feature, which adds permissive CORS so the SPA (different origin) can call the API.
- Compiles the shim with `GE_DEV`, enabling the core-library hot reload (see the C shim above).
- `just dev` runs `vite dev` (UI hot reload) plus a watch loop that relinks the core (`make the_golden_eye_core`) when `obs2/rust` changes and then pings `GE_RELOAD_FIFO`; the shim hot-reloads the rebuilt core — so editing the UI *or* the Rust code reloads live without restarting OBS.

### Where things live

- `obs2/cv_templates/` — PNG templates for the level matcher (mission/part/difficulty labels, digits, colons). Authored at native capture resolution; the matcher in `cv.rs` searches scales in `SCALES` to recover from non-native capture sizes. Templates are language-suffixed (`en-`, `jp-`); language selected via `GE_LANG` env (default `en`).
- `obs2/vendor/obs/` — vendored OBS headers, populated by `just obs-headers` (sparse clone of `obsproject/obs-studio` at the version in `justfile`'s `obs_version`).
- `obs2/rust/src/bin/test_match.rs` — standalone CLI that runs `cv::match_level` on a single PNG and emits JSON. Used by the test harness in `test/`.

## Commands

All top-level workflows go through `just` (driven by `justfile`). `set dotenv-load` means `.env` is auto-loaded.

### Setup (run once)

```sh
just setup            # vendor OBS headers, npm install (root + obs2/browser), download llama-server + GGUF models
```

System deps: `rustup`, `nodejs` (version in `.nvmrc`); macOS also needs `brew install cmake wget simde`, OBS installed in `/Applications`. On macOS the build resolves `libclang.dylib` from either Xcode or Command Line Tools — `xcode-select --install` if neither is present. OpenCV is statically compiled from source via `just opencv-static` (no Homebrew opencv needed).

### Building & running obs2 (the plugin)

```sh
just make             # cmake Debug build (no dev redirect)
just make-release     # cmake Release build
just obs              # build + launch OBS with the plugin loaded
just obs-flatpak      # Linux: launch Flatpak OBS with the plugin bind-mounted (needed for YouTube OAuth plugin)
just dev              # cmake Debug w/ BROWSER_DEV=ON + run `vite dev` in parallel + launch OBS
just clean            # remove vendored headers, node_modules, ge_rust.h, build dirs, esp32 .pio, cargo target
```

### Tests

```sh
just test             # release-build obs2, then run frame regression tests
just test-watch       # same in watch mode
```

The test harness (`test/frames.test.ts`) iterates over PNGs in `test/screenshots/`, shells out to the `test_match` Rust binary (`obs2/rust/target/release/test_match`), and compares against expected values derived from the filename (encoding `lang - kind - level - difficulty`). Results are written to `test_results.json`. There's currently **one runner** configured in `test/runners.ts`; add more by appending to that list.

To run the matcher on a single screenshot directly:

```sh
GE_LANG=en obs2/rust/target/release/test_match path/to/shot.png
```

### Running obs (the legacy Node implementation)

```sh
just run              # `npm run obs` — launches the TUI; expects OBS WebSocket on ws://localhost:4455
just repl             # node REPL with project loaded
just upload <dir>     # YouTube upload helper
```

Node scripts use `--experimental-strip-types`, so `.ts` files run directly without a build step.

### Frontend (obs2/browser) — running standalone

```sh
cd obs2/browser
npm run dev           # vite dev server on :5173 (used by `just dev`)
npm run check         # svelte-check (TS + Svelte)
npm run lint          # prettier --check
npm run format        # prettier --write
npm run test:unit     # vitest
npm run test:e2e      # playwright
```

### Rust crate — running standalone

```sh
cd obs2/rust
cargo build --release           # also rebuilds when BROWSER_BUNDLE env changes
cargo build --release --bin test_match
cargo test
```

`BROWSER_BUNDLE` must point to an existing file for the library/binary to compile (because of the `include_str!`). Normally CMake sets it; when invoking cargo directly, either run a CMake build first or set `BROWSER_BUNDLE` to any HTML file you have.

## Environment variables

`.env` is loaded by `just` and read by both implementations.

- `GE_LANG` — `en` or `jp`. Picks the template set.
- `OBS_PASSWORD` — used by the legacy `obs/index.ts` to authenticate against OBS WebSocket.
- `DISCORD_WEBHOOK_URL`, `DISCORD_MESSAGE_NAME` — read by Rust `Config` for the "now streaming" Discord notification. Resolved once at startup and logged.
- `GE_CV_THREADS` — caps OpenCV's internal thread pool in the `test_match` binary (benchmarking hook).
- `OPENCV_INCLUDE_DIR` / `OPENCV_LIB_DIR` — needed on Arch Linux per the README.

## Conventions worth knowing

- **Don't manually edit `obs2/ge_rust.h`** — it's regenerated by `build.rs` via cbindgen on every Rust build.
- **Frame format on the C↔Rust boundary is BGRA** (`width * height * 4`). The C bridge `malloc`s; the Rust side must `free` via the FFI'd `libc::free`. The `test_match` CLI converts loaded PNGs from BGR to BGRA before calling into the matcher so it matches the in-OBS code path.
- **Screen-detection ordering matters** in `obs/matcher.ts` — `EndLevelFailed` is a subset of `EndLevelComplete` for the `mission-status` template, so the order in the `matchers` array is load-bearing.
- **`opencv4nodejs` doesn't work in worker threads**, which is why the Node implementation uses a child-process pool (`MatcherProcessPool` in `obs/matcher.ts`) instead.
- The HTTP server uses tower middleware composed top-down (first added = outermost); the axum router composes bottom-up. Both files note this — preserve the ordering when adding layers.
