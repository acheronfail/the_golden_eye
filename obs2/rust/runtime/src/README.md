# Plugin code ownership

Start with the feature that owns the behavior. Each feature keeps its state and workflows together;
HTTP and native callbacks translate external requests into calls to those owners.

## Find a mechanism

| Behavior                                                 | Start here                                                                                                                                         |
| -------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Per-frame gameplay workflow                              | [run_monitoring/session.rs](run_monitoring/session.rs), `RunSession::process_match`                                                                |
| Run start, cancellation, failure, and completion         | [run_monitoring/run_detection.rs](run_monitoring/run_detection.rs), `RunDetection::on_frame`                                                       |
| In-game timer transitions                                | [run_monitoring/in_game_timer.rs](run_monitoring/in_game_timer.rs), `InGameTimer`                                                                  |
| Monitor startup and shutdown                             | [run_monitoring/lifecycle.rs](run_monitoring/lifecycle.rs), `RunMonitor`                                                                           |
| Replay save, trimming, and completion                    | [run_monitoring/clip_saving.rs](run_monitoring/clip_saving.rs) and [replay_buffer.rs](run_monitoring/replay_buffer.rs)                             |
| Run listing, imports, retention requests, and edits      | [run_library/mod.rs](run_library/mod.rs), `RunLibrary`                                                                                             |
| YouTube connection and uploads                           | [youtube_uploads/oauth.rs](youtube_uploads/oauth.rs) and [upload.rs](youtube_uploads/upload.rs), methods on `YoutubeUploadStore`                   |
| Stream notification start and stop                       | [streaming_notifications/mod.rs](streaming_notifications/mod.rs), `StreamNotifier`                                                                 |
| Update checks and safe application                       | [plugin_updates/lifecycle.rs](plugin_updates/lifecycle.rs), `PluginUpdates`; package handling in [installation.rs](plugin_updates/installation.rs) |
| Diagnostic capture and one-shot matching                 | [diagnostics](diagnostics/mod.rs)                                                                                                                  |
| Feature construction and shared dependencies             | [app/startup.rs](app/startup.rs), `build_state`                                                                                                    |
| Settings persistence and application                     | [settings.rs](settings.rs) and [app/settings.rs](app/settings.rs)                                                                                  |
| Browser snapshots and discrete events                    | [app/publication.rs](app/publication.rs) and [app/events.rs](app/events.rs)                                                                        |
| OBS calls, frame capture, and dock integration           | [obs](obs/mod.rs)                                                                                                                                  |
| Native browser, folder, and file operations              | [desktop](desktop/mod.rs)                                                                                                                          |
| HTTP decoding, response mapping, and WebSocket transport | [http](http/mod.rs)                                                                                                                                |

[Run monitoring](run_monitoring/README.md) describes the main gameplay workflow in more detail. The
[frontend map](../../../browser/src/lib/README.md) describes UI ownership.

## Boundaries

`app` constructs owners and coordinates the few operations that cross features, such as applying
settings. Feature workflows receive specific dependencies, not `AppState`. Shared browser output
belongs to `app`, with feature-specific payload types and publication rules defined by their owners.

Within a workflow, use ordinary calls: the frame worker calls the session, which calls recording and
timing logic. One-off outputs use the typed `AppEvent` broadcast channel; retained browser state
uses one watch channel. There is no dynamic subscriber registry or internal per-frame event router.
The watch channel is the only retained snapshot, so mutation and publication happen together.

The browser socket store owns connection and dispatch. Settings and YouTube stores own their state
updates and notification policies. Shared frontend stores remain shared when several features use
them; component-private behavior stays with its component.

Pure game rules, CV matching, clip metadata, media processing, catalog storage, and settings models
remain in their existing Rust crates. Catalog SQL and retention transactions stay with `ge_catalog`.
`lib.rs` owns the runtime and native entry points; `obs` owns OBS adapters. The C core and resident
loader retain their existing loading and reload responsibilities. Firmware and frame regression
tools remain independent projects.

See [the ownership review](OWNERSHIP_REVIEW.md) for costs, validation, and behavior changes.
