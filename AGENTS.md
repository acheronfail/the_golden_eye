# AGENTS.md

This file provides guidance to agents when working with code in this repository.

## What This Project Is

"The Golden Eye" is a native OBS plugin that watches a GoldenEye 007 (N64) capture, detects level start/end/result screens, parses level info and times from the on-screen overlays, and drives OBS recording/replay handling around runs. Discord/YouTube integration can post and later edit a streaming notification when an OBS YouTube stream starts/stops.

The repo contains:

- `obs2/` - the active native OBS plugin, driven by `just obs` / `just dev`.
- `frame_tests/` - Node-based frame regression harness for the Rust matcher CLI.
- `docs/` - installation, troubleshooting, and internal development guides.

This repo is v2-only. Do not add a root Node application, OBS WebSocket control path, downloaded model runtime, or helper-script stack unless the user explicitly asks for a separate new implementation.

## Architecture

The runtime is a layered stack glued together by CMake:

1. **C loader** (`obs2/loader/plugin.c`) - the library OBS actually loads. Kept deliberately minimal and not updatable by the plugin's auto-update flow; anything that can live in Rust/runtime instead does. It contains no core logic beyond OBS-module lifecycle and path resolution: `obs_module_load` rejects duplicate loaded plugin copies, resolves the bundled core library relative to the resident loader (overridable via `GE_CORE_LIB`), derives staging beside that core, and opens it via `loader/reload.c`. `obs_module_post_load` forwards to the core's `ge_core_post_load()`. `loader/reload.c` owns only the core's dlopen/close/reload mechanics (always via a fresh temp copy, never the canonical path directly, so no platform's loader can hand back a stale cached image) plus a dedicated reload worker thread: Rust asks the core to request a reload (`ge_core_trigger_reload`, see below), which wakes this worker to swap in a staged core -- closing the old core fully before opening the new one (the core binds a fixed TCP port, so old and new can never run at once), rolling back to the running version if the new one fails to load, and only replacing the canonical core after a successful swap. It passes the canonical core and staging paths into the core but never interprets package data or OBS data layouts. See `obs2/loader/tests/` for fixture-driven tests of this mechanism (no OBS/Rust dependency).
2. **C core** (`obs2/core/core.c`, `obs_bridge.c`) - the heavy library the loader hosts. `ge_core_load` stores the loader's reload-request callback, calls `ge_runtime_start()` (returns `false` if the HTTP port fails to bind, which the loader treats as a load failure), registers OBS frontend callbacks, connects OBS source-change signals, and pushes an initial source snapshot. Frontend events drive stream notifications, replay-buffer lifecycle state, replay-save completion, and source refreshes. `ge_core_post_load` refreshes sources and ensures the OBS custom browser dock. `ge_core_commit_update` commits Rust's pending runtime-data transaction after the loader replaces the canonical core. `ge_core_unload` disconnects signals/callbacks and stops Rust. `ge_core_trigger_reload` is called from Rust (`plugin_updates/installation.rs`) once an update is staged and safe to apply; it wakes the loader's reload worker. `obs_bridge.c` exposes OBS helpers to Rust, including source-frame rendering to BGRA.
3. **Rust settings crate** (`obs2/rust/settings/`, crate `ge_settings`) - the dependency-light source of truth for persisted setting types, defaults, and normalization. Its exporter generates `browser/src/lib/generated/settings.ts`; the main Rust crate wraps it with OBS-dependent projections.
4. **Rust game crate** (`obs2/rust/game/`, crate `ge_game`) - pure GoldenEye level, difficulty, target-time, and intro-timing rules shared by matching, recording, catalog, and HTTP code.
5. **Rust CV crate** (`obs2/rust/cv/`, crate `ge_cv`) - the OpenCV-backed level/time, black-frame, and watch matcher. It also owns the standalone `test_match` and `annotate_match` tools, which do not link the plugin runtime.
6. **Rust clip crate** (`obs2/rust/clip/`, crate `ge_clip`) - dependency-light clip metadata, run-status, and ROM-version types shared by the catalog, recording, HTTP, and media code.
7. **Rust media crate** (`obs2/rust/media/`, crate `ge_media`) - the FFmpeg-backed clip duration, trimming, remuxing, and container-tag adapter for `ge_clip` metadata.
8. **Rust catalog crate** (`obs2/rust/catalog/`, crate `ge_catalog`) - the SQLite-backed run catalog, retention and filesystem coordination, statistics, monitor sessions, and durable YouTube associations.
9. **Rust staticlib** (`obs2/rust/runtime/`, crate `ge_runtime`) - owns a global `tokio::Runtime` inside a `Mutex<Option<ServerHandle>>`. FFI entry points are `extern "C"` and spawn work onto the runtime without blocking the caller. `app/` constructs feature owners and coordinates shared publication; `run_monitoring/` owns gameplay monitoring, run detection, the in-game timer, and replay-buffer save/trim/rename behavior; `run_library/` owns run browsing and editing workflows; `youtube_uploads/` owns OAuth and uploads; `streaming_notifications/` posts/edits Discord webhooks; `plugin_updates/` checks releases and stages/applies updates; `diagnostics/` owns diagnostic capture and matching; `settings.rs` persists app settings; `obs/` and `desktop/` provide native adapters; `http/` is the Axum transport. During a reload, Rust provisionally replaces the complete OBS module-data directory before startup reports success. The loader commits that transaction only after replacing the canonical core; unloading first restores the previous data.
10. **Axum HTTP server** - listens on loopback only (`127.0.0.1:31337`). It exposes OBS recording, replay-buffer status, monitoring (including WebSocket), settings, folder picking/validation, runs media/reveal/rename, source, screenshot, matcher, OAuth callback, and SPA routes under `/api/v1`, `/oauth/callback`, and `/`.
11. **SvelteKit SPA** (`obs2/browser/`) - Svelte 5 + Tailwind v4 + Vite, built with `@sveltejs/adapter-static`. Output `build/index.html` is embedded into the Rust binary at compile time.

### Build Coupling

The CMake build (`obs2/CMakeLists.txt`) wires these dependencies as a strict chain:

- `obs2/rust/Cargo.toml` defines the `catalog`, `clip`, `runtime`, `cv`, `game`, `media`, and `settings` workspace members with one lockfile and target directory.
- `browser_contracts` exports the settings and API contracts before the browser build.
- `browser_build` runs `npm run build` in `obs2/browser/`, producing the HTML bundle at `$BROWSER_BUNDLE` (normally `obs2/browser/build/index.html`). `GE_REUSE_HOST_BUILD_INPUTS=ON` reuses an existing bundle and validates it when `browser_build` runs.
- `rust_build` depends on `browser_build`. `cargo build --lib --bins` (the staticlib the core links, plus the `test_match`/`annotate_match` bins the `frame_tests/` harness needs -- deliberately not `--all-targets`, so a normal build doesn't compile the integration-test/bench crates; `cargo test` builds those on demand in the test recipes) runs with `BROWSER_BUNDLE`, `GE_PLUGIN_VERSION`, and `GE_BROWSER_DEV_URL` set; the Rust crate embeds the bundle via `include_str!`. `build.rs` also runs `cbindgen` and writes `obs2/core/ge_runtime.h` (used by `core.c`). `GE_REUSE_HOST_BUILD_INPUTS=ON` reuses the existing staticlib/header and validates them when `rust_build` runs.
- The plugin target depends on `rust_libs` (an `IMPORTED STATIC` library pointing at `target/{debug,release}/libge_runtime.a`).

A failed frontend build stops the chain before cargo runs. Do not bypass this dependency chain.

**Dev mode** (`-DBROWSER_DEV=ON`, used by `just dev`):

- Skips the SPA build and embeds a tiny redirect HTML pointing at `http://localhost:31336` (the Vite dev server).
- Enables the Rust `dev` feature, which adds permissive CORS so the SPA can call the API from a different origin.
- Runs `vite dev` plus a watch loop that relinks the core (`make golden_core`) when either Rust workspace crate changes, then hot-reloads it into the running OBS session -- but via the production auto-update pipeline, not a dev-only FIFO (that watcher was removed when the loader was minimized): `obs2/scripts/dev.py` stages the freshly rebuilt core into `.ge_update_staged/` and POSTs to `/api/v1/updates/apply`. `plugin_updates/lifecycle.rs`'s background auto-apply loop treats dev builds (`cfg!(feature = "dev")`) as always opted in and polls faster, so it picks the staged rebuild up on its own if that POST is momentarily refused (e.g. a monitor session is active). No OBS restart needed either way.

### Where Things Live

- `obs2/cv_templates/` - PNG templates for the level matcher. Templates are language-suffixed (`en-`, `jp-`); `test_match` takes the language as a CLI argument. CMake copies these into the built plugin data layout (`Contents/Resources/cv_templates` on macOS, `data/cv_templates` on Linux/Windows).
- `obs2/vendor/obs/` - vendored OBS headers, populated by `just obs-headers`.
- `obs2/vendor/opencv-static/` and `obs2/vendor/ffmpeg-static/` - static dependency prefixes built by `just opencv-static` and `just ffmpeg-static`.
- `obs2/rust/cv/src/bin/test_match.rs` - standalone CLI that runs the matcher on a single PNG and emits JSON. Used by the test harness in `frame_tests/`.
- `obs2/rust/runtime/src/http/routes/` - Axum route handlers. Keep route-specific behavior here instead of bloating `http/mod.rs`.
- `obs2/rust/runtime/src/app/` - feature construction, cross-feature settings application, and browser publication. Feature workflows receive explicit dependencies instead of `AppState`; HTTP routes decode requests and map results. See `obs2/rust/runtime/src/README.md` for the ownership map.
- `obs2/rust/runtime/src/settings.rs` - persisted settings in `settings.json` under the OS app config directory (`~/Library/Application Support/The Golden Eye` on macOS, `$XDG_CONFIG_HOME/the-golden-eye` or `~/.config/the-golden-eye` on Linux).
- `obs2/rust/settings/` - the canonical `ge_settings` model/defaults and TypeScript exporter.
- `obs2/rust/runtime/src/run_monitoring/` - automatic run capture. Start with `session.rs` for the workflow, `run_detection.rs` for run transitions, `in_game_timer.rs` for timer rules, and `lifecycle.rs` for starting/stopping OBS capture. See its `README.md` for the responsibility map.
- `obs2/rust/catalog/` - the SQLite-backed **run catalog** (`runs.sqlite`, stored next to `settings.json`). `run_catalog.rs` owns a single `Mutex<Connection>` and exposes all catalog ops; SQL stays in `runs.rs` and `meta.rs`. Every finalized run has a durable row, whether or not its optional clip still exists. See the Run Catalog conventions below.
- `obs2/rust/clip/` - the canonical `ClipMetadata`, `RunStatus`, and `RomVersion` model.
- `obs2/rust/media/` - FFmpeg-backed clip probing, trimming, remuxing, and tag read/write.
- `obs2/loader/` - the thin loader's C sources (`plugin.c`, `dynlib.c`, `reload.c`) and its standalone tests (`obs2/loader/tests/`, run via `just test-loader`; no OBS/Rust toolchain needed).
- `obs2/core/` - the core's C sources (`core.c`, `obs_bridge.c`) and the cbindgen-generated `ge_runtime.h`.
- `frame_tests/` - frame regression harness with its own `package.json`; scripts use Node's `--experimental-strip-types`.

## Commands

All top-level workflows go through `just` (driven by `justfile`).

See [CONTRIBUTING.md](CONTRIBUTING.md) for the system prerequisites and command reference.

```sh
just setup            # install project dependencies, Rust tools, and Chromium
just dev              # OBS with browser and core hot reload
just storybook        # component previews without OBS
just make             # debug build; uses the OBS Flatpak SDK on Linux
just make-release     # release build; uses the same SDK on Linux
just fmt              # format only; no builds or checks
just fmt-check        # non-mutating format check
just check            # regenerate contracts; check formatting, Clippy, and browser types
just test             # all six test suites
just test-browser     # browser unit/component tests
just test-storybook   # Chromium story tests
just test-rust        # Rust unit tests
just test-integration # fake-OBS integration tests
just test-loader      # native load/reload fixtures
just test-cv          # release build and captured-frame regressions
```

`just clean` removes build/test outputs and the generated C header, but keeps dependencies.
`just clean-deps` also removes npm dependencies; `just clean-all` also removes vendored native builds.
Neither command deletes tracked contracts or fixtures. Run `just setup` after dependency cleanup.

The harness in `frame_tests/` derives expected matcher results from fixture filenames.
See [frame_tests/README.md](frame_tests/README.md) for filters and watch mode.

For direct Cargo commands, run `just configure-release` and build the `browser_build` CMake target
first. Source `obs2/build/rust-cargo-env.sh` for the version, bundle, and native dependency settings.
Run Cargo from `obs2/rust/`, which selects the pinned toolchain. Prefer the `just` recipes for normal work.

## Environment Variables

- `BROWSER_BUNDLE`, `GE_PLUGIN_VERSION`, `GE_BROWSER_DEV_URL` - build-time inputs consumed by Rust/CMake. Build through `just` unless you need direct cargo commands.
- `GE_CORE_LIB` - optional loader override for the core library path.
- `GE_SERVER_PORT` - overrides the local HTTP server port (default from `obs2/server-port.txt`; integration tests use 31338).
- `GE_CV_THREADS` - caps OpenCV's internal thread pool in `test_match` for benchmarking.
- `GE_CV_BENCH`, `GE_CV_BENCH_WARM`, `GE_CV_DEBUG`, `GE_CV_TIMING` - matcher benchmarking/debugging hooks.
- `GE_DISABLE_BROWSER_DOCK`, `GE_BROWSER_DOCK_URL` - opt out of or override automatic OBS custom browser dock setup.
- `GE_BROWSER_WS_LOG` - enables browser dev-tools logging of app WebSocket traffic when truthy (`1`, `true`, `yes`, `on`).
- `GE_UPDATE_CHECK_URL` - overrides the GitHub releases API URL the startup update check hits; used by tests to point at a local mock server.
- `RUST_LOG` - tracing filter; defaults to crate-level debug in debug builds and info in release builds plus tower_http.

Discord notification settings are no longer read from `DISCORD_WEBHOOK_URL`; they are stored in the persisted app settings and exposed through `/api/v1/settings`.

## Conventions

- Keep comments concise: no comment (doc or inline) should exceed 3 lines. If an explanation needs more, tighten the wording rather than adding lines.
- Every new frontend component must include or update a Storybook story under `obs2/browser/src/stories/`, covering its default and materially distinct states.
- Organize `obs2/browser/src/lib` by ownership using `app/`, `features/<name>/`, `ui/`, `stores/`, and `developer/`; follow `obs2/browser/src/lib/README.md`.
- Keep feature code flat by default. Use a PascalCase component capsule only when a component has meaningful private children or implementation modules.
- Keep component-private types in the component, rune-based controllers in `*.svelte.ts`, pure logic in specifically named modules, and tests beside their subjects.
- Avoid catch-all `components`, `controllers`, `effects`, `types`, `utils`, and `helpers` directories. Mirror source ownership under `src/stories/`.
- Use Tailwind v4 utilities for all representable frontend styling. Put shared design-system utilities and theme or animation definitions in `obs2/browser/src/routes/layout.css`; do not add component `<style>` blocks or static inline styles when Tailwind can express the result.
- When a value is calculated at runtime, pass only that value through a CSS custom property and use Tailwind utilities for the actual layout, color, or animation property.
- Rust runtime environment variable reads belong in `obs2/rust/runtime/src/config/*.rs`, using `EnvVar` from `config/mod.rs`, with re-exports from `config/mod.rs`. Do not read runtime env vars ad hoc from routes/tasks.
- Do not manually edit `obs2/core/ge_runtime.h`; it is regenerated by `build.rs` via cbindgen on every Rust build.
- Frame format on the C/Rust boundary is BGRA (`width * height * 4`). The C bridge `malloc`s; Rust must free via the FFI'd `libc::free`.
- The `test_match` CLI converts loaded PNGs from BGR to BGRA before calling into the matcher so it matches the in-OBS code path.
- The HTTP server uses tower middleware composed top-down (first added = outermost); the axum router composes bottom-up. Preserve that ordering when adding layers.
- Follow the existing FFI/bridge patterns when adding OBS calls from Rust routes or tasks; be especially careful about OBS API thread/lifetime expectations.
- Preserve the replay-buffer event flow: Rust requests save/start/stop through OBS APIs, and `core.c` forwards replay lifecycle/saved events back to Rust so saves wait on actual OBS completion instead of polling.
- Keep `obs2/loader/plugin.c` minimal and free of OBS-unrelated logic. The loader is intentionally outside the auto-update payload, so loader/core contract changes require a new updater number and manual installation; source-only renames do not. `obs2/loader/reload.c` has no OBS dependency by design (so `obs2/loader/tests/` can exercise it standalone) -- don't add one.
- The reload-request callback passed into `ge_core_load` (`ge_request_reload_fn`) must only ever wake the loader's reload worker thread and return immediately; it's invoked from a call stack still inside the core being reloaded, so it must never itself touch a dlopen handle or call back into the core. See `obs2/loader/reload.h`.

### Run Catalog (`obs2/rust/catalog/`)

- SQLite is authoritative for durable run history. Clip container tags mirror the run ID, metadata, and retention state so an existing clip can be re-imported after a catalog reset. YouTube associations remain attached to the run row while its clip exists.
- Every finalized run is inserted after stats voting and before clip processing. Stable IDs use the high-precision completion timestamp, `levelNumber`, and difficulty; never persist matcher `mission`/`part` identity.
- The configurable recent history keeps the newest 1–20 rows visible. After a new clip is saved, cleanup deletes only clips for older `pending` rows; reads and settings changes never delete files. `kept` clips survive cleanup; deleted/expired clips leave their metadata-only rows in history.
- Default listing reads all catalog rows, including rows without clips. A full disk rescan remains opt-in through the Runs reload action; externally added tagged clips are imported as `kept`.
- Keep SQL in `runs.rs` and `meta.rs`; `run_catalog.rs` orchestrates catalog and filesystem operations while holding the single connection mutex where retention races matter. There is intentionally one storage implementation.
- Schema version 3 is current. Opening schema 1 resets its tables and reseeds tagged clips as `kept`. Schema 2 migrates to schema 3. Preserve migrations for later schema changes.
