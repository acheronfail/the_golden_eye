use std::sync::Arc;

use tokio::sync::{broadcast, watch};

use super::{AppEvent, SharedStateStore};

pub struct AppStateInner {
    /// YouTube OAuth credentials/history plus retained upload state.
    pub youtube: crate::youtube_uploads::YoutubeUploadStore,
    pub notifications: crate::streaming_notifications::StreamNotifier,
    /// Owns monitor startup, capture resources, and shutdown.
    pub monitor: Arc<crate::run_monitoring::RunMonitor>,
    pub updates: Arc<crate::plugin_updates::PluginUpdates>,
    /// The single retained app/session state object. New browser clients receive
    /// this on connect, then every retained-state change as a fresh snapshot.
    pub snapshot: SharedStateStore,
    /// One-off app events broadcast to connected clients (e.g. a clip being
    /// saved). Discrete events are not retained for late joiners.
    pub event_tx: broadcast::Sender<AppEvent>,
    /// Independent diagnostic capture; stopped and joined before core unload.
    pub frame_dump: crate::diagnostics::FrameDumper,
    /// Signals when OBS has emitted `OBS_FRONTEND_EVENT_FINISHED_LOADING` and
    /// frontend replay-buffer APIs are safe to query.
    pub frontend_ready_tx: watch::Sender<bool>,
    /// SQLite-backed index of saved run clips.
    pub run_catalog: std::sync::Arc<ge_catalog::run_catalog::RunCatalog>,
    pub runs: Arc<crate::run_library::RunLibrary>,
    /// Plugin-owned user settings, loaded from and persisted to JSON.
    pub settings: Arc<crate::settings::SettingsStore>,
    /// Successful commit time, used for a brief notice to newly connected clients.
    pub update_committed_at: std::sync::Mutex<Option<std::time::Instant>>,
}

pub type AppState = Arc<AppStateInner>;

impl AppStateInner {
    pub fn update_applied_event(&self) -> AppEvent {
        let settings = self.settings.get();
        let version = settings.last_known_update_version.as_deref().map(|v| v.strip_prefix('v').unwrap_or(v));
        let release_url =
            if version == Some(crate::PLUGIN_VERSION) { settings.last_known_update_release_url.clone() } else { None };
        AppEvent::UpdateApplied { version: crate::PLUGIN_VERSION.to_owned(), release_url }
    }
}
