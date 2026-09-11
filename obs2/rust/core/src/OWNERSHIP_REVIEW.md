# Ownership refactor review

The goal is to make behavior easy to find, follow, and audit. The unfamiliar-reader feedback on the
monitoring reorganization justified extending that approach to the other plugin capabilities. This
review records the implemented boundaries and costs; passing tests alone does not establish that new
readers will find every workflow easier.

## What changed

| Question                                | Result                                                                                                                                                                                                                 |
| --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Where are the three central mechanisms? | `RunDetection::on_frame`, `InGameTimer`, and `RunMonitor::start` / `stop` live in explicitly named files under `run_monitoring`. `RunSession::process_match` shows their per-frame coordination.                       |
| Who owns feature state?                 | `RunMonitor`, `FrameDumper`, `RunLibrary`, `StreamNotifier`, `YoutubeUploadStore`, and `PluginUpdates` own their lifecycle state and operations. The app container holds these owners instead of exposing their locks. |
| What belongs to HTTP?                   | Request decoding, HTTP validation and error mapping, task scheduling, media streaming, and WebSocket transport. Feature workflows and native adapters no longer live in routes.                                        |
| How are dependencies visible?           | `app/startup.rs` constructs feature owners with explicit dependencies. Runtime feature modules have no `AppState` or `crate::http` dependency.                                                                         |
| How does state reach the browser?       | One watch channel retains the app snapshot. Feature owners implement their publication rules. Typed broadcast events carry discrete outputs.                                                                           |
| How does the browser respond?           | The socket store dispatches messages; settings and YouTube stores own their state changes and notification policies.                                                                                                   |
| What remains independently owned?       | Existing game, CV, clip, media, catalog, and settings crates; OBS/desktop adapters; C core and shim; firmware and regression tools.                                                                                    |

The per-frame path is worker → session → recorder → run detection, with clocks and timer updated by
the session. The previous internal `MonitorEvent` and `RecordingEvent` dispatch stages are gone.
There is no new general event-bus framework, subscriber registry, or lock around the session.

Some files remain substantial: run-library operations and upload processing contain real workflows.
Their public entry points precede supporting details, and their names describe the responsibility.
Splitting every helper into another file would increase the navigation burden without improving
ownership.

## Behavior changes and limits

Most changes relocate code and replace application-container access with explicit dependencies.
These changes go beyond relocation:

- Snapshot mutation and publication now use the same retained watch value. Previously a mutex-owned
  snapshot was copied into a separate watch value after releasing the mutex, allowing concurrent
  publications to arrive out of order. The separate mutex and retained copy are removed. No-op
  writes still do not notify subscribers.
- Core shutdown now explicitly stops and joins standalone diagnostic frame capture before unload, as
  it already did for the monitor worker.
- Successful run-library mutations publish their own catalog-change events. The normal response
  ordering is preserved; an operation that finishes after its HTTP caller stops awaiting it can
  still publish its successful change.

Monitor startup and shutdown retain their existing lock scope: stop takes the active handle before
awaiting cleanup, so `is_active` becomes false while teardown is still in progress. Concurrent start
and stop are not serialized through all cleanup. This refactor does not claim to fix that overlap or
the updater's separate activity observations.

Run-detection and timer rules, replay permits and restart ordering, catalog retention transactions,
clip identity, and native loading contracts retain their existing implementation. This work does not
introduce new enum-based run or monitor state machines.

## Size cost

Net source lines in the working tree, including tests, comments, and blank lines; documentation is
excluded. Rust covers `obs2/rust/**/*.rs`; frontend covers TypeScript, Svelte, and CSS under
`obs2/browser/src`. These totals include untracked destination files, so moves are counted
correctly.

| Baseline                                                        | Rust net lines | Frontend net lines |
| --------------------------------------------------------------- | -------------: | -----------------: |
| Current branch HEAD before these uncommitted changes, `fae15db` |           +569 |                +12 |
| Earlier timer baseline, `fae142b`                               |           +651 |                +12 |
| Fetched master, `origin/master` (`c8c8401`, PR #179 squash)     |         +1,312 |                +83 |
| Local `master` (`8614d26`, older than fetched master)           |         +1,314 |               +172 |

This is an ownership improvement with a size cost. Explicit construction, owner operations, domain
errors, and regression coverage account for additional code; removed routers and shared-container
access offset only part of it. The useful review is whether a reader can identify the owner and
follow the workflow without tracing unrelated features, rather than whether the line count falls.

## Validation

- `just make`: native Debug plugin, browser bundle, contract export, and Rust build passed. The
  linker reports local dependency objects targeting macOS 26.2 against a 26.0 target.
- `just test-rust --lib -- --skip keyring_store_round_trips_tokens`: 247 passed. The OS keyring
  round-trip test is excluded from this unattended run.
- `just test-integration`: 41 passed. Fake-OBS coverage includes monitor lifecycle, replay saves, settings,
  update application, catalog operations, and mocked YouTube OAuth/uploads.
- `npm run check`: zero errors and warnings. `npm run test`: 430 tests passed, including Storybook
  in Chromium.
- Browser-generated contracts and the generated C header are unchanged. `git diff --check` passes.

New targeted checks cover concurrent snapshot publication, standalone frame-dump cleanup on core
shutdown, and monitoring shutdown with its published reason after an unexpected replay-buffer stop.
These checks exercise the boundaries affected by this work; they do not replace real-OBS testing or
an unfamiliar reader tracing the other newly reorganized workflows.

A useful follow-up review is to trace a run from frame observation through saved clip, a timer pause
and resume, an external replay stop, a failed OAuth connection, and a staged update. Each should
have a recognizable feature entry point and a visible workflow inside that feature.
