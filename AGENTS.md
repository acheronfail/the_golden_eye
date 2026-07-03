# AGENTS.md

This file provides guidance to agents when working with code in this repository.

## What This Project Is

"The Golden Eye" is a native OBS plugin that watches a GoldenEye 007 (N64) capture, detects level start/end/result screens, parses level info and times from the on-screen overlays, and drives OBS recording/replay handling around runs. Discord/YouTube integration can post and later edit a streaming notification when an OBS YouTube stream starts/stops.

The repo contains:

- `obs2/` - the active native OBS plugin, driven by `just obs` / `just dev`.
- `esp32-input-monitor/` - independent PlatformIO firmware that sniffs N64 controller DATA lines and exposes state over WebSocket. It has its own `README.md` and `pio` build; it is not wired into the OBS plugin.
- `test/` - Node-based frame regression harness for the Rust matcher CLI.
- root helpers like `viewer.html`, `sample_clip.mov`, `screenshots/`, `TODO.md`, and `BUGS.md` are support/debug artifacts, not a separate application stack.

This repo is v2-only. Do not add a root Node application, OBS WebSocket control path, downloaded model runtime, or helper-script stack unless the user explicitly asks for a separate new implementation.

## Architecture

The runtime is a layered stack glued together by CMake:

1. **C shim** (`obs2/plugin.c`) - the library OBS actually loads. It contains no core logic; `obs_module_load` rejects duplicate loaded plugin copies, resolves the bundled core library relative to the loaded shim (overridable via `GE_CORE_LIB`), configures `GE_CV_TEMPLATE_DIR` from OBS's module data path or the bundled template path when unset, `dlopen`s the core with `RTLD_NOW | RTLD_LOCAL`, then calls `ge_core_load()`. `obs_module_post_load` forwards to `ge_core_post_load()`. In dev builds (`GE_DEV`, set when `BROWSER_DEV=ON`) it dlopens a throwaway copy so rebuilt images load, then uses `dev_reload.c` to hot-reload the core when `GE_RELOAD_FIFO` is pinged.
2. **C core** (`obs2/core.c`, `obs_bridge.c`) - the heavy library the shim hosts. `ge_core_load` calls `ge_rust_start()`, registers OBS frontend callbacks, connects OBS source-change signals, and pushes an initial source snapshot. Frontend events drive stream notifications, replay-buffer lifecycle state, replay-save completion, and source refreshes. `ge_core_post_load` refreshes sources and ensures the OBS custom browser dock. `ge_core_unload` disconnects signals/callbacks and stops Rust. `obs_bridge.c` exposes OBS helpers to Rust, including source-frame rendering to BGRA.
3. **Rust staticlib** (`obs2/rust/`, crate `ge_rust`) - owns a global `tokio::Runtime` inside a `Mutex<Option<ServerHandle>>`. FFI entry points are `extern "C"` and spawn work onto the runtime without blocking the caller. `cv.rs` contains the level/time matcher; `recording.rs` owns replay-buffer save/trim/rename behavior; `settings.rs` persists app settings; `stream_notifier.rs` posts/edits Discord webhooks; `browser_dock.rs` manages the OBS dock; `http/` is the Axum app.
4. **Axum HTTP server** - listens on `0.0.0.0:31337`. It exposes OBS recording, replay-buffer status, monitoring (including WebSocket), settings, folder picking/validation, runs media/reveal/rename, source, screenshot, matcher, OAuth callback, and SPA routes under `/api/v1`, `/oauth/callback`, and `/`.
5. **SvelteKit SPA** (`obs2/browser/`) - Svelte 5 + Tailwind v4 + Vite, built with `@sveltejs/adapter-static`. Output `build/index.html` is embedded into the Rust binary at compile time.

### Build Coupling

The CMake build (`obs2/CMakeLists.txt`) wires these dependencies as a strict chain:

- `browser_build` runs `npm run build` in `obs2/browser/`, producing the HTML bundle at `$BROWSER_BUNDLE` (normally `obs2/browser/build/index.html`). `GE_REUSE_HOST_BUILD_INPUTS=ON` reuses an existing bundle and validates it when `browser_build` runs.
- `rust_build` depends on `browser_build`. `cargo build --all-targets` runs with `BROWSER_BUNDLE`, `GE_PLUGIN_VERSION`, and `GE_BROWSER_DEV_URL` set; the Rust crate embeds the bundle via `include_str!`. `build.rs` also runs `cbindgen` and writes `obs2/ge_rust.h` (used by `core.c`). `GE_REUSE_HOST_BUILD_INPUTS=ON` reuses the existing staticlib/header and validates them when `rust_build` runs.
- The plugin target depends on `rust_libs` (an `IMPORTED STATIC` library pointing at `target/{debug,release}/libge_rust.a`).

A failed frontend build stops the chain before cargo runs. Do not bypass this dependency chain.

**Dev mode** (`-DBROWSER_DEV=ON`, used by `just dev`):

- Skips the SPA build and embeds a tiny redirect HTML pointing at `http://localhost:5173` (the Vite dev server).
- Enables the Rust `dev` feature, which adds permissive CORS so the SPA can call the API from a different origin.
- Compiles the shim with `GE_DEV`, enabling core-library hot reload.
- Runs `vite dev` plus a watch loop that relinks the core (`make golden_core`) when `obs2/rust/src` or `obs2/rust/Cargo.toml` changes and then pings `GE_RELOAD_FIFO`.

### Where Things Live

- `obs2/cv_templates/` - PNG templates for the level matcher. Templates are language-suffixed (`en-`, `jp-`); `test_match` takes the language as a CLI argument. CMake copies these into the built plugin layout (`Contents/Resources/cv_templates` on macOS, `cv_templates/` beside the Linux plugin library).
- `obs2/vendor/obs/` - vendored OBS headers, populated by `just obs-headers`.
- `obs2/vendor/opencv-static/` and `obs2/vendor/ffmpeg-static/` - static dependency prefixes built by `just opencv-static` and `just ffmpeg-static`.
- `obs2/rust/src/bin/test_match.rs` - standalone CLI that runs `cv::match_level` on a single PNG and emits JSON. Used by the test harness in `test/`.
- `obs2/rust/src/http/routes/` - Axum route handlers. Keep route-specific behavior here instead of bloating `http/mod.rs`.
- `obs2/rust/src/settings.rs` - persisted settings in `settings.json` under the OS app config directory (`~/Library/Application Support/The Golden Eye` on macOS, `$XDG_CONFIG_HOME/the-golden-eye` or `~/.config/the-golden-eye` on Linux).
- `obs2/rust/src/recording.rs` - replay-buffer coordination, clip extraction, filename templates, and output-path defaults.
- `test/` - frame regression harness with its own `package.json`; scripts use Node's `--experimental-strip-types`.

## Commands

All top-level workflows go through `just` (driven by `justfile`).

### Setup

```sh
just setup            # vendor OBS headers, build static OpenCV/FFmpeg, npm install obs2/browser + test
```

System deps: `rustup`, `nodejs` (version in `.nvmrc`), `just`, `wget`; macOS also needs `xcode-select --install`, `brew install cmake simde nasm`, and OBS installed in `/Applications`. Linux development targets the OBS Flatpak for packaging/running; install the SDK shown by `flatpak info --show-sdk com.obsproject.Studio`. x86_64 hosts need `nasm` for static FFmpeg.

### Building And Running

```sh
just make             # cmake Debug build (no dev redirect)
just make-release     # cmake Release build
just obs              # macOS: build + launch OBS; Linux: Flatpak OBS with plugin bind-mounted
just dev              # Debug build + Vite dev server + core hot reload + OBS
just make-package     # release package zip in obs2/build*/dist
just install          # install the packaged plugin into the platform OBS plugin dir
just uninstall        # remove it from that plugin dir
just fmt              # frontend prettier, nightly rustfmt, clang-format C/H
just clean            # remove generated build/vendor/dependency artifacts
```

Linux-specific build internals are `just make-release-flatpak` and `just _flatpak-build <target>`; there is no `just obs-flatpak` target now.

### Tests

```sh
just test             # release-build obs2, then run frame regression tests
just test-watch       # same in watch mode
just test-rust        # Rust unit tests; requires an existing browser bundle and static FFmpeg prefix
```

The test harness (`test/frames.test.ts`) iterates over PNGs in `test/screenshots-*`, shells out to `obs2/rust/target/release/test_match`, and compares against expected values derived from the filename.

To run the matcher on a single screenshot directly:

```sh
obs2/rust/target/release/test_match en path/to/shot.png
```

### Frontend

```sh
cd obs2/browser
npm run dev
npm run check
npm run lint
npm run format
npm run format:repo
npm run test
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

`BROWSER_BUNDLE` and `GE_PLUGIN_VERSION` must be set for direct cargo builds because Rust uses `env!`/`include_str!`. Normally CMake sets them. When invoking cargo directly, either run a CMake build first or export `BROWSER_BUNDLE` to an existing HTML file and `GE_PLUGIN_VERSION` to a semantic version. For FFmpeg-linked tests/builds, mirror the justfile's static FFmpeg setup (`PKG_CONFIG_PATH=$FFMPEG_PREFIX/lib/pkgconfig`).

## Environment Variables

- `BROWSER_BUNDLE`, `GE_PLUGIN_VERSION`, `GE_BROWSER_DEV_URL` - build-time inputs consumed by Rust/CMake. Build through `just` unless you need direct cargo commands.
- `GE_CORE_LIB` - optional shim override for the core library path.
- `GE_CV_TEMPLATE_DIR` - optional template directory override. OBS runtime defaults to the templates bundled by CMake next to/inside the plugin.
- `GE_CV_THREADS` - caps OpenCV's internal thread pool in `test_match` for benchmarking.
- `GE_CV_BENCH`, `GE_CV_BENCH_WARM`, `GE_CV_DEBUG`, `GE_CV_TIMING` - matcher benchmarking/debugging hooks.
- `GE_RELOAD_FIFO` - dev-mode FIFO pinged by `just dev` to trigger core hot reload.
- `GE_DISABLE_BROWSER_DOCK`, `GE_BROWSER_DOCK_URL` - opt out of or override automatic OBS custom browser dock setup.
- `RUST_LOG` - tracing filter; defaults to crate-level debug in debug builds and info in release builds plus tower_http.

Discord notification settings are no longer read from `DISCORD_WEBHOOK_URL`; they are stored in the persisted app settings and exposed through `/api/v1/settings`.

## Conventions

- Do not manually edit `obs2/ge_rust.h`; it is regenerated by `build.rs` via cbindgen on every Rust build.
- Frame format on the C/Rust boundary is BGRA (`width * height * 4`). The C bridge `malloc`s; Rust must free via the FFI'd `libc::free`.
- The `test_match` CLI converts loaded PNGs from BGR to BGRA before calling into the matcher so it matches the in-OBS code path.
- The HTTP server uses tower middleware composed top-down (first added = outermost); the axum router composes bottom-up. Preserve that ordering when adding layers.
- Follow the existing FFI/bridge patterns when adding OBS calls from Rust routes or tasks; be especially careful about OBS API thread/lifetime expectations.
- Preserve the replay-buffer event flow: Rust requests save/start/stop through OBS APIs, and `core.c` forwards replay lifecycle/saved events back to Rust so saves wait on actual OBS completion instead of polling.
