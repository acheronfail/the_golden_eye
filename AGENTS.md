# AGENTS.md

This file provides guidance to agents when working with code in this repository.

## What This Project Is

"The Golden Eye" is a native OBS plugin that watches a GoldenEye 007 (N64) capture, detects level start/end screens, parses level info from the on-screen stats overlay, and drives OBS recording/replay handling around runs. Discord/YouTube integration posts a "now streaming" notification when an OBS YouTube stream starts.

The repo contains:

- `obs2/` - the active native OBS plugin, driven by `just obs` / `just dev`.
- `esp32-input-monitor/` - independent PlatformIO firmware that sniffs N64 controller DATA lines and exposes state over WebSocket. It has its own `README.md` and `pio` build; it is not wired into the OBS plugin.

This repo is v2-only. Do not add a root Node application, OBS WebSocket control path, downloaded model runtime, or helper-script stack unless the user explicitly asks for a separate new implementation.

## Architecture

The runtime is a layered stack glued together by CMake:

1. **C shim** (`obs2/plugin.c`) - the library OBS actually loads. It contains no core logic; `obs_module_load` resolves the bundled core library relative to the loaded shim (overridable via the `GE_CORE_LIB` env var), sets `GE_CV_TEMPLATE_DIR` to the bundled templates when the env var is not already set, `dlopen`s the core with `RTLD_NOW`, then calls `ge_core_load()`. In dev builds (`GE_DEV`, set when `BROWSER_DEV=ON`) it dlopens a throwaway copy so rebuilt images load, then uses `dev_reload.c` to hot-reload the core when `GE_RELOAD_FIFO` is pinged.
2. **C core** (`obs2/core.c`, `obs_bridge.c`) - the heavy library the shim hosts. `ge_core_load` calls `ge_rust_start()` and registers the frontend callback; on `OBS_FRONTEND_EVENT_STREAMING_STARTED` it calls `ge_stream_notifier_start()`. `ge_core_unload` tears it all down. `obs_bridge.c` exposes helpers to Rust, including `ge_obs_get_source_frame`, which renders an OBS source to a BGRA buffer.
3. **Rust staticlib** (`obs2/rust/`, crate `ge_rust`) - owns a global `tokio::Runtime` inside a `Mutex<Option<ServerHandle>>`. FFI entry points (`ge_rust_start`, `ge_rust_stop`, `ge_stream_notifier_start/stop`) are `extern "C"` and spawn work onto the runtime without blocking the caller. `cv.rs` contains the level/time matcher; `stream_notifier.rs` posts Discord webhooks; `http/` is the Axum app.
4. **Axum HTTP server** - listens on `0.0.0.0:31337`. It exposes OBS recording, replay, monitoring, screenshot, matcher, source, OAuth, and SPA routes under `/api/v1`, `/oauth/callback`, and `/`.
5. **SvelteKit SPA** (`obs2/browser/`) - Svelte 5 + Tailwind v4 + Vite, built with `@sveltejs/adapter-static`. Output `build/index.html` is embedded into the Rust binary at compile time.

### Build Coupling

The CMake build (`obs2/CMakeLists.txt`) wires these dependencies as a strict chain:

- `browser_build` runs `npm run build` in `obs2/browser/`, producing `obs2/browser/build/index.html`.
- `rust_build` depends on `browser_build`. `cargo build` runs with `BROWSER_BUNDLE` set to that path; the Rust crate embeds it via `include_str!`. `build.rs` also runs `cbindgen` and writes `obs2/ge_rust.h` (used by `plugin.c`).
- The plugin target depends on `rust_libs` (an `IMPORTED STATIC` library pointing at `target/{debug,release}/libge_rust.a`).

A failed frontend build stops the chain before cargo runs. Do not bypass this dependency chain.

**Dev mode** (`-DBROWSER_DEV=ON`, used by `just dev`):

- Skips the SPA build and embeds a tiny redirect HTML pointing at `http://localhost:5173` (the Vite dev server).
- Enables the Rust `dev` feature, which adds permissive CORS so the SPA can call the API from a different origin.
- Compiles the shim with `GE_DEV`, enabling core-library hot reload.
- Runs `vite dev` plus a watch loop that relinks the core (`make golden_core`) when `obs2/rust` changes and then pings `GE_RELOAD_FIFO`.

### Where Things Live

- `obs2/cv_templates/` - PNG templates for the level matcher. Templates are language-suffixed (`en-`, `jp-`); language is selected via `GE_LANG` (default `en`). CMake copies these into the built plugin layout (`Contents/Resources/cv_templates` on macOS, `cv_templates/` beside the Linux plugin library).
- `obs2/vendor/obs/` - vendored OBS headers, populated by `just obs-headers`.
- `obs2/vendor/opencv-static/` and `obs2/vendor/ffmpeg-static/` - static dependency prefixes built by `just opencv-static` and `just ffmpeg-static`.
- `obs2/rust/src/bin/test_match.rs` - standalone CLI that runs `cv::match_level` on a single PNG and emits JSON. Used by the test harness in `test/`.
- `test/` - Node-based frame regression harness with its own `package.json`.

## Commands

All top-level workflows go through `just` (driven by `justfile`). `set dotenv-load` means `.env` is auto-loaded.

### Setup

```sh
just setup            # vendor OBS headers, build static OpenCV/FFmpeg, npm install obs2/browser + test
```

System deps: `rustup`, `nodejs` (version in `.nvmrc`), `just`, `wget`; macOS also needs `xcode-select --install`, `brew install cmake simde nasm`, and OBS installed in `/Applications`. x86_64 hosts need `nasm` for the static FFmpeg build.

### Building And Running

```sh
just make             # cmake Debug build (no dev redirect)
just make-release     # cmake Release build
just obs              # build + launch OBS with the plugin loaded
just obs-flatpak      # Linux: launch Flatpak OBS with the plugin bind-mounted
just dev              # Debug build + Vite dev server + core hot reload + OBS
just clean            # remove generated build/vendor/dependency artifacts
```

### Tests

```sh
just test             # release-build obs2, then run frame regression tests
just test-watch       # same in watch mode
just test-rust        # Rust unit tests; requires an existing browser bundle
```

The test harness (`test/frames.test.ts`) iterates over PNGs in `test/screenshots-*`, shells out to `obs2/rust/target/release/test_match`, and compares against expected values derived from the filename.

To run the matcher on a single screenshot directly:

```sh
GE_LANG=en obs2/rust/target/release/test_match path/to/shot.png
```

### Frontend

```sh
cd obs2/browser
npm run dev
npm run check
npm run lint
npm run format
npm run test:unit
npm run test:e2e
```

### Rust Crate

```sh
cd obs2/rust
cargo build --release
cargo build --release --bin test_match
cargo test
```

`BROWSER_BUNDLE` must point to an existing file for the library/binary to compile because of `include_str!`. Normally CMake sets it; when invoking cargo directly, either run a CMake build first or set `BROWSER_BUNDLE` to any HTML file you have.

## Environment Variables

`.env` is loaded by `just`.

- `GE_LANG` - `en` or `jp`. Picks the template set.
- `DISCORD_WEBHOOK_URL`, `DISCORD_MESSAGE_NAME` - read by Rust `Config` for the Discord stream notification. Resolved once at startup and logged.
- `GE_CV_THREADS` - caps OpenCV's internal thread pool in `test_match` for benchmarking.
- `GE_CV_TEMPLATE_DIR` - optional template directory override. OBS runtime defaults to the templates bundled by CMake next to/inside the plugin.

## Conventions

- Do not manually edit `obs2/ge_rust.h`; it is regenerated by `build.rs` via cbindgen on every Rust build.
- Frame format on the C/Rust boundary is BGRA (`width * height * 4`). The C bridge `malloc`s; Rust must free via the FFI'd `libc::free`.
- The `test_match` CLI converts loaded PNGs from BGR to BGRA before calling into the matcher so it matches the in-OBS code path.
- The HTTP server uses tower middleware composed top-down (first added = outermost); the axum router composes bottom-up. Preserve that ordering when adding layers.
