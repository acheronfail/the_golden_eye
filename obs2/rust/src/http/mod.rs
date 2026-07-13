mod routes;

use std::net::SocketAddr;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use axum::Router;
use axum::error_handling::HandleErrorLayer;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{get, post};
pub(crate) use routes::monitor::stop_monitor;
use serde::Serialize;
use tokio::net::{TcpListener, TcpSocket};
use tokio::sync::{Mutex, broadcast, oneshot, watch};
use tower::ServiceBuilder;
use tower_http::BoxError;

use crate::cv::LevelMatch;

const API_REQUEST_TIMEOUT: Duration = Duration::from_secs(20 * 60);

pub struct AppStateInner {
    /// Holds the sender end of a one-shot channel while an OAuth flow is in
    /// progress. The `/oauth/callback` route fires it when the code arrives.
    pub oauth_pending: Mutex<Option<oneshot::Sender<String>>>,
    /// The Discord "now streaming" message posted when a stream starts, kept so
    /// the stop handler can edit it in place rather than posting a new message.
    pub stream_message: Mutex<Option<StreamMessage>>,
    /// The currently running monitor, if any. Enforces a single monitor at a
    /// time: `/api/v1/monitor/start` fails while this is `Some`.
    pub monitor: std::sync::Mutex<Option<routes::monitor::MonitorHandle>>,
    /// Latest `LevelMatch` from the running monitor (`None` when stopped). A
    /// `watch` channel: only sent when the matched state changes (ignoring
    /// `runtime_ms`), and retained so a mid-run client sees the current match.
    pub match_tx: watch::Sender<Option<LevelMatch>>,
    /// One-off monitor events broadcast to connected WebSocket clients (e.g. a
    /// clip being saved). A `broadcast` channel: discrete events, nothing retained
    /// for late joiners. Send errors (no subscribers) are ignored at call sites.
    pub event_tx: broadcast::Sender<MonitorEvent>,
    /// Latest recorder phase from the running monitor. This is retained so a
    /// page reload or second browser can see "recording" / "saving" / etc.
    /// immediately, instead of waiting for the next transition.
    pub recording_state: RecordingStateStore,
    /// Developer-only, in-memory switch that makes the live monitor include
    /// matcher regions and annotation sets in its debug/info payloads. This is
    /// intentionally not part of persisted settings.
    pub monitor_annotations_enabled: AtomicBool,
    /// Latest OBS video-source list, broadcast to browser clients whenever OBS
    /// reports source creation/removal/update/rename. Retained so a page load
    /// receives the current source picker state immediately.
    pub source_tx: watch::Sender<Vec<routes::sources::Source>>,
    /// Latest plugin update detected at startup. Retained so a browser dock that
    /// connects after the network check finishes still gets the sticky notice.
    pub update_tx: watch::Sender<Option<crate::updates::PluginUpdate>>,
    /// Plugin-owned user settings, loaded from and persisted to JSON.
    pub settings: crate::settings::SettingsStore,
    /// `Some(start instant)` if this core load followed a successful update apply
    /// (see `crate::WAS_RELOADED`), so a client connecting within a grace period
    /// gets a one-off "plugin updated" notice (see `routes::monitor::handle_socket`).
    pub reloaded_at: Option<std::time::Instant>,
}

/// A Discord webhook message we posted and may later edit.
pub struct StreamMessage {
    pub id: String,
    pub broadcast_url: String,
    pub webhook_url: String,
}

/// Messages pushed to app WebSocket clients, internally tagged by `type` for
/// the SPA. Some variants ride watch channels (latest-wins, replayed on
/// connect); others ride `event_tx` (one-off, only to connected clients).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MonitorEvent {
    /// Sent once on connect: the build id of the SPA this backend serves. The
    /// SPA compares it against its own served build and reloads on mismatch, so
    /// a stale tab picks up the new frontend. See [`routes::index::BUILD_ID`].
    Version {
        #[serde(rename = "buildId")]
        build_id: String,
    },
    /// The current OBS video-source list changed.
    Sources { sources: Vec<routes::sources::Source> },
    /// The matched on-screen state changed; carries the current match.
    Match(LevelMatch),
    /// The recorder's run state changed (a run began, was cancelled, saw a
    /// failure screen, had its save scheduled, or returned to idle). Distinct
    /// from `RecordingSaved`, which reports the final written clip.
    RecordingState { status: Option<RecordingStatus> },
    /// The source showed a ROM language-specific marker. The monitor uses this
    /// to keep its active matcher and recording metadata aligned automatically.
    LanguageDetected { lang: String },
    /// Periodic monitor throughput telemetry. `processed_fps` is the slowest
    /// completed-frame cadence observed since the last telemetry push;
    /// `source_fps` is the OBS video cadence driving capture callbacks.
    MonitorFps(MonitorFps),
    /// A run's clip save was scheduled and will fire after the post-run padding.
    RecordingSavePending(RecordingSavePending),
    /// A run's clip was saved out of the replay buffer and trimmed.
    RecordingSaved(RecordingSaved),
    /// Monitoring stopped, either from a user request or an external OBS event.
    MonitorStopped { reason: MonitorStoppedReason },
    /// The settings JSON file changed on disk and was reloaded successfully.
    SettingsReloaded {
        #[serde(rename = "configPath")]
        config_path: String,
        settings: crate::settings::AppSettings,
    },
    /// The settings JSON file changed on disk but could not be parsed or read.
    SettingsInvalid {
        #[serde(rename = "configPath")]
        config_path: String,
        error: String,
    },
    /// A newer plugin release is available on GitHub.
    UpdateAvailable(crate::updates::PluginUpdate),
    /// Sent once when a client connects shortly after this core was loaded via
    /// an applied update (dev hot-reload or a real auto-update), so the SPA
    /// can show a one-off "plugin updated" notice. See `AppStateInner::reloaded_at`.
    UpdateApplied {
        version: String,
        /// GitHub release page for `version`, but only when the persisted
        /// `last_known_update_version` matches the running version (i.e. this is
        /// the update just applied). `None` otherwise, to avoid a wrong link.
        #[serde(rename = "releaseUrl", skip_serializing_if = "Option::is_none")]
        release_url: Option<String>,
    },
    /// A newer release was found but downloading/verifying/staging it failed
    /// (e.g. an unwritable install directory), so no update is queued up to
    /// apply. One-off, delivered via `event_tx` -- see `updates::check_for_updates_now`.
    UpdateStagingFailed { error: String },
}

/// Why the backend stopped an active monitor. Serialized as a plain string
/// inside [`MonitorEvent::MonitorStopped`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MonitorStoppedReason {
    /// A client requested `/api/v1/monitor/stop`.
    UserStopped,
    /// OBS reported that its replay buffer stopped while monitoring was active.
    ReplayBufferStopped,
}

/// Monitor throughput sampled by the worker thread and pushed to the frontend
/// while monitoring is active.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorFps {
    pub processed_fps: f64,
    pub source_fps: f64,
}

/// A transition in the recorder's per-run state, broadcast so the SPA can
/// reflect where a run is in its lifecycle. Serialized as a plain string (e.g.
/// `"started"`) inside the enclosing [`MonitorEvent::RecordingState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordingStatus {
    /// A run began: the replay-buffer clip's start was anchored.
    Started,
    /// The active run was abandoned before reaching the stats screen (the user
    /// returned to the level-select grid), so nothing is saved for it.
    Cancelled,
    /// The "mission failed" report screen was seen during the active run. The run
    /// still ends normally (at the stats screen or on backing out) and the clip is
    /// saved.
    Failed,
    /// The "mission aborted" report screen was seen during the active run (a
    /// failure, like [`RecordingStatus::Failed`], distinguished so the UI can name
    /// why the run ended).
    Aborted,
    /// The "killed in action" report screen was seen during the active run
    /// (another failure variant, distinguished for the UI).
    Kia,
    /// The mission-complete report screen was reached: the run succeeded.
    /// Emitted once per run -- on first sight, or to clear an earlier-flagged
    /// failure (so the SPA can leave the "failed" state).
    Complete,
    /// A *completed* run backed out of the report screen to the level grid,
    /// bypassing the stats screen. The clip is still saved and a
    /// [`MonitorEvent::RecordingSaved`] follows. (A failed run does this normally.)
    StatsSkipped,
    /// A failed run reached an ending screen, but the active recording config
    /// says not to save it (failed-run saving disabled, or the run time is
    /// shorter than the configured minimum failed-run length).
    FailedDiscarded,
    /// A run ended at the stats screen (or, via `StatsSkipped`, the report
    /// screen): a save has been scheduled and will fire a few seconds later. A
    /// [`MonitorEvent::RecordingSaved`] follows once the clip is written.
    SavePending,
}

/// Retained recorder phase shared by the monitor worker, status endpoint, and
/// WebSocket clients. Transient phases are cleared here so the backend owns the
/// same lifecycle the UI displays.
#[derive(Clone)]
pub struct RecordingStateStore {
    tx: watch::Sender<Option<RecordingStatus>>,
    state: Arc<StdMutex<RecordingStateInner>>,
}

struct RecordingStateInner {
    status: Option<RecordingStatus>,
    generation: u64,
}

impl RecordingStateStore {
    const CANCELLED_LINGER: Duration = Duration::from_secs(2);
    const SAVE_TIMEOUT: Duration = Duration::from_secs(30);

    pub fn new(tx: watch::Sender<Option<RecordingStatus>>) -> Self {
        RecordingStateStore { tx, state: Arc::new(StdMutex::new(RecordingStateInner { status: None, generation: 0 })) }
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<RecordingStatus>> {
        self.tx.subscribe()
    }

    pub fn current(&self) -> Option<RecordingStatus> {
        self.lock_state().status
    }

    pub fn set(&self, status: RecordingStatus) {
        let generation = {
            let mut state = self.lock_state();
            state.generation += 1;
            state.status = Some(status);
            self.tx.send_replace(state.status);
            state.generation
        };

        match status {
            RecordingStatus::Cancelled | RecordingStatus::FailedDiscarded => {
                self.clear_after(generation, Self::CANCELLED_LINGER);
            }
            RecordingStatus::SavePending | RecordingStatus::StatsSkipped => {
                self.clear_after(generation, Self::SAVE_TIMEOUT);
            }
            _ => {}
        }
    }

    pub fn clear(&self) {
        let mut state = self.lock_state();
        state.generation += 1;
        state.status = None;
        self.tx.send_replace(state.status);
    }

    pub fn clear_if_save_pending(&self) {
        let mut state = self.lock_state();
        if matches!(state.status, Some(RecordingStatus::SavePending | RecordingStatus::StatsSkipped)) {
            state.generation += 1;
            state.status = None;
            self.tx.send_replace(state.status);
        }
    }

    fn clear_after(&self, generation: u64, duration: Duration) {
        let store = self.clone();
        let spawned = std::thread::Builder::new().name("ge-recording-state-timeout".to_owned()).spawn(move || {
            std::thread::sleep(duration);
            store.clear_if_generation(generation);
        });
        if let Err(err) = spawned {
            tracing::error!("failed to spawn recording-state timeout thread: {err}");
        }
    }

    fn clear_if_generation(&self, generation: u64) {
        let mut state = self.lock_state();
        if state.generation == generation {
            state.generation += 1;
            state.status = None;
            self.tx.send_replace(state.status);
        }
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, RecordingStateInner> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Details of a clip save that has been scheduled after a run ending was seen,
/// pushed to WebSocket clients as a [`MonitorEvent::RecordingSavePending`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSavePending {
    /// Identifier shared with the matching [`RecordingSaved`] event.
    pub save_id: u64,
    /// Seconds until OBS replay-buffer save is requested.
    pub save_in_secs: f64,
    /// Expected trimmed clip length, before replay-buffer duration clamping.
    pub estimated_duration_secs: f64,
    /// Whether a failure screen was seen during the run.
    pub failed: bool,
    /// Final run status used for naming/metadata.
    pub status: String,
    /// Human-readable level name, or "unknown" if the matcher could not resolve it.
    pub level: String,
    /// GoldenEye campaign level number, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level_number: Option<i32>,
    /// Human-readable difficulty label, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<String>,
    /// Run time read from the stats screen, in seconds, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_secs: Option<i32>,
    /// Target time read from the stats screen, in seconds, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_time_secs: Option<i32>,
    /// Best time read from the stats screen, in seconds, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_time_secs: Option<i32>,
    /// The stats-screen match the clip will be named from, when one was seen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<LevelMatch>,
}

/// Details of a clip saved out of the replay buffer at the end of a run, pushed
/// to WebSocket clients as a [`MonitorEvent::RecordingSaved`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSaved {
    /// Identifier shared with the matching [`RecordingSavePending`] event.
    pub save_id: u64,
    /// Absolute path to the trimmed clip written for the run.
    pub path: String,
    /// The full replay-buffer file OBS saved, before trimming.
    pub replay_path: String,
    /// Length of the trimmed clip, in seconds.
    pub duration_secs: f64,
    /// Whether a failure screen was seen during the run.
    pub failed: bool,
    /// The stats-screen match the clip was named from, when one was seen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<LevelMatch>,
}

pub type AppState = Arc<AppStateInner>;

pub const SERVER_PORT: u16 = 31337;
pub const OAUTH_CALLBACK_PATH: &str = "/oauth/callback";

pub fn collect_sources() -> Vec<routes::sources::Source> {
    routes::sources::collect_sources()
}

/// Logs each request as it arrives and again once a response is produced.
async fn log_requests(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    tracing::debug!(%method, %path, "request received");

    let start = std::time::Instant::now();
    let response = next.run(req).await;
    let elapsed = start.elapsed();

    let status = response.status();
    tracing::debug!(%method, %path, %status, ?elapsed, "request sent");
    response
}

/// Binds the listening socket synchronously so callers (e.g. `ge_rust_start`)
/// learn immediately whether the port bound. Must run inside a `runtime.enter()`
/// guard; sets `SO_REUSEADDR` so the port can rebind across a reload (TIME_WAIT).
pub fn bind_listener() -> std::io::Result<TcpListener> {
    let addr = SocketAddr::from(([0, 0, 0, 0], SERVER_PORT));
    let socket = TcpSocket::new_v4()?;
    socket.set_reuseaddr(true)?;
    socket.bind(addr)?;
    socket.listen(1024)
}

pub async fn serve(listener: TcpListener, shutdown: oneshot::Receiver<()>, state: AppState) -> anyhow::Result<()> {
    // Build middleware stack

    // NOTE: tower composes middleware from top to bottom; i.e., the first added is the first to be run
    let middleware = ServiceBuilder::new()
        // Added first so it's outermost: logs every request and sees the final
        // status, including timeouts handled by the layers below.
        .layer(axum::middleware::from_fn(log_requests))
        .layer(HandleErrorLayer::new(|error: BoxError| async move {
            if error.is::<tower::timeout::error::Elapsed>() {
                Ok(StatusCode::REQUEST_TIMEOUT)
            } else {
                Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Unhandled internal error: {error}")))
            }
        }))
        // A native folder picker can legitimately stay open while the user
        // navigates the OS dialog; keep the local API timeout above that path.
        .timeout(API_REQUEST_TIMEOUT);

    // Build application router

    // NOTE: axum composes middleware from bottom to top; i.e., the last added is the first to be run
    let app = Router::new()
        .route("/api/v1/record/start", post(routes::record::handle_start))
        .route("/api/v1/record/stop", post(routes::record::handle_stop))
        .route("/api/v1/replay-buffer/status", get(routes::record::handle_replay_status))
        .route("/api/v1/monitor/start", post(routes::monitor::handle_start))
        .route("/api/v1/monitor/stop", post(routes::monitor::handle_stop))
        .route("/api/v1/monitor/status", get(routes::monitor::handle_status))
        .route("/api/v1/monitor/ws", get(routes::monitor::handle_ws))
        .route("/api/v1/settings", get(routes::settings::handle_get).put(routes::settings::handle_put))
        .route("/api/v1/settings/status", get(routes::settings::handle_status))
        .route("/api/v1/settings/reset", post(routes::settings::handle_reset))
        .route("/api/v1/folders/pick", post(routes::folders::handle_pick))
        .route("/api/v1/folders/validate", post(routes::folders::handle_validate))
        .route("/api/v1/files/reveal", post(routes::files::handle_reveal))
        .route("/api/v1/updates/open", post(routes::updates::handle_open))
        .route("/api/v1/updates/check", post(routes::updates::handle_check_now))
        .route("/api/v1/updates/download", post(routes::updates::handle_download_now))
        .route("/api/v1/updates/status", get(routes::updates::handle_status))
        .route("/api/v1/updates/apply", post(routes::updates::handle_apply_now))
        .route(
            "/api/v1/runs",
            get(routes::runs::handle_list)
                .delete(routes::runs::handle_delete)
                .patch(routes::runs::handle_update_metadata),
        )
        .route("/api/v1/runs/stream", get(routes::runs::handle_stream))
        .route("/api/v1/runs/rename", post(routes::runs::handle_rename))
        .route("/api/v1/runs/thumbnail", get(routes::runs::handle_thumbnail))
        .route("/api/v1/runs/video", get(routes::runs::handle_video))
        .route("/api/v1/sources", get(routes::sources::handler))
        .route("/api/v1/screenshot", get(routes::screenshot::handler))
        .route("/api/v1/match", post(routes::matcher::handler))
        .route("/api/v1/match/annotations", post(routes::matcher::handle_annotations))
        .route(OAUTH_CALLBACK_PATH, get(routes::oauth::handle_callback))
        .route("/", get(routes::index::handler))
        // fallback for frontend spa
        .fallback(get(routes::index::handler))
        .layer(middleware.into_inner());

    // In dev the SPA is served by the Vite dev server on a different origin, so
    // its fetches to this API are cross-origin. Allow them with permissive CORS.
    // Only compiled in for dev builds (CMake BROWSER_DEV=ON) — never in release.
    #[cfg(feature = "dev")]
    let app = app.layer(tower_http::cors::CorsLayer::permissive());

    let app = app.with_state(state.clone());

    tracing::info!("listening on {}", listener.local_addr()?);
    let _ = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            // Resolve when a shutdown is requested, or if the sender is dropped.
            let _ = shutdown.await;
        })
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_version_event_uses_frontend_field_name() {
        let event = MonitorEvent::Version { build_id: "abc123".to_owned() };
        let json = serde_json::to_value(event).unwrap();

        assert_eq!(json["type"], "version");
        assert_eq!(json["buildId"], "abc123");
        assert!(json.get("build_id").is_none());
    }

    #[test]
    fn update_available_event_flattens_update_payload() {
        let event = MonitorEvent::UpdateAvailable(crate::updates::PluginUpdate {
            current_version: "1.0.0".to_owned(),
            latest_version: "1.1.0".to_owned(),
            release_url: "https://github.com/acheronfail/the_golden_eye/releases/tag/v1.1.0".to_owned(),
        });
        let json = serde_json::to_value(event).unwrap();

        assert_eq!(json["type"], "updateAvailable");
        assert_eq!(json["currentVersion"], "1.0.0");
        assert_eq!(json["latestVersion"], "1.1.0");
        assert_eq!(json["releaseUrl"], "https://github.com/acheronfail/the_golden_eye/releases/tag/v1.1.0");
    }

    #[test]
    fn monitor_recording_state_event_can_clear_status() {
        let event = MonitorEvent::RecordingState { status: None };
        let json = serde_json::to_value(event).unwrap();

        assert_eq!(json["type"], "recordingState");
        assert!(json["status"].is_null());
    }

    #[test]
    fn language_detected_event_uses_frontend_field_names() {
        let event = MonitorEvent::LanguageDetected { lang: "en".to_owned() };
        let json = serde_json::to_value(event).unwrap();

        assert_eq!(json["type"], "languageDetected");
        assert_eq!(json["lang"], "en");
    }

    #[test]
    fn monitor_fps_event_uses_frontend_field_names() {
        let event = MonitorEvent::MonitorFps(MonitorFps { processed_fps: 59.5, source_fps: 60.0 });
        let json = serde_json::to_value(event).unwrap();

        assert_eq!(json["type"], "monitorFps");
        assert_eq!(json["processedFps"], 59.5);
        assert_eq!(json["sourceFps"], 60.0);
        assert!(json.get("processed_fps").is_none());
    }

    #[test]
    fn recording_save_pending_event_uses_frontend_field_names() {
        let event = MonitorEvent::RecordingSavePending(RecordingSavePending {
            save_id: 7,
            save_in_secs: 5.0,
            estimated_duration_secs: 74.5,
            failed: false,
            status: "complete".to_owned(),
            level: "Dam".to_owned(),
            level_number: Some(1),
            difficulty: Some("Agent".to_owned()),
            time_secs: Some(69),
            target_time_secs: Some(120),
            best_time_secs: None,
            stats: None,
        });
        let json = serde_json::to_value(event).unwrap();

        assert_eq!(json["type"], "recordingSavePending");
        assert_eq!(json["saveId"], 7);
        assert_eq!(json["saveInSecs"], 5.0);
        assert_eq!(json["estimatedDurationSecs"], 74.5);
        assert_eq!(json["timeSecs"], 69);
        assert!(json.get("bestTimeSecs").is_none());
    }

    #[test]
    fn recording_saved_event_uses_frontend_field_names() {
        let event = MonitorEvent::RecordingSaved(RecordingSaved {
            save_id: 7,
            path: "/tmp/clip.mp4".to_owned(),
            replay_path: "/tmp/replay.mp4".to_owned(),
            duration_secs: 74.5,
            failed: false,
            stats: None,
        });
        let json = serde_json::to_value(event).unwrap();

        assert_eq!(json["type"], "recordingSaved");
        assert_eq!(json["saveId"], 7);
        assert_eq!(json["path"], "/tmp/clip.mp4");
        assert_eq!(json["replayPath"], "/tmp/replay.mp4");
        assert_eq!(json["durationSecs"], 74.5);
        assert!(json.get("stats").is_none());
    }

    #[test]
    fn sources_event_uses_frontend_field_name() {
        let event = MonitorEvent::Sources {
            sources: vec![routes::sources::Source {
                name: "N64 Capture".to_owned(),
                id: "av_capture_input".to_owned(),
            }],
        };
        let json = serde_json::to_value(event).unwrap();

        assert_eq!(json["type"], "sources");
        assert_eq!(json["sources"][0]["name"], "N64 Capture");
        assert_eq!(json["sources"][0]["id"], "av_capture_input");
    }

    #[test]
    fn recording_state_store_retains_state_without_receivers() {
        let (tx, rx) = watch::channel(None);
        let store = RecordingStateStore::new(tx);
        drop(rx);

        store.set(RecordingStatus::Started);
        assert_eq!(store.current(), Some(RecordingStatus::Started));

        store.set(RecordingStatus::SavePending);
        store.set(RecordingStatus::Started);
        store.clear_if_save_pending();
        assert_eq!(store.current(), Some(RecordingStatus::Started));

        store.clear();
        assert_eq!(store.current(), None);
    }

    #[test]
    fn monitor_stopped_event_uses_frontend_field_names() {
        let event = MonitorEvent::MonitorStopped { reason: MonitorStoppedReason::ReplayBufferStopped };
        let json = serde_json::to_value(event).unwrap();

        assert_eq!(json["type"], "monitorStopped");
        assert_eq!(json["reason"], "replayBufferStopped");

        let event = MonitorEvent::MonitorStopped { reason: MonitorStoppedReason::UserStopped };
        let json = serde_json::to_value(event).unwrap();

        assert_eq!(json["type"], "monitorStopped");
        assert_eq!(json["reason"], "userStopped");
    }
}
