use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::{Mutex, broadcast, oneshot, watch};

use super::{ReplayBufferStatus, routes};
use crate::cv::{BlackFrameSignal, LevelMatch};
pub use crate::in_game_timer::{LevelTimerPhase, LevelTimerStartReason};
use crate::recording::{RecordingStateStore, RecordingStatus};

pub struct AppStateInner {
    /// Holds the sender end of a one-shot channel while an OAuth flow is in
    /// progress. The `/oauth/callback` route fires it when the code arrives.
    pub oauth_pending: Mutex<Option<PendingOAuth>>,
    /// YouTube OAuth credentials/history plus retained upload state.
    pub youtube: crate::youtube::YoutubeUploadStore,
    /// The Discord "now streaming" message posted when a stream starts, kept so
    /// the stop handler can edit it in place rather than posting a new message.
    pub stream_message: Mutex<Option<StreamMessage>>,
    /// The currently running monitor, if any. Enforces a single monitor at a
    /// time; serializable monitor state lives in `snapshot`.
    pub monitor: std::sync::Mutex<Option<crate::monitor::MonitorHandle>>,
    /// The single retained app/session state object. New browser clients receive
    /// this on connect, then every retained-state change as a fresh snapshot.
    pub snapshot: SharedStateStore,
    /// One-off app events broadcast to connected clients (e.g. a clip being
    /// saved). Discrete events are not retained for late joiners.
    pub event_tx: broadcast::Sender<AppEvent>,
    /// Latest recorder phase from the running monitor, with generation-aware
    /// timeout clearing. Writes also update `snapshot.recording_state`.
    pub recording_state: RecordingStateStore,
    /// Retained per-clip replay save pipeline state for debug visibility.
    pub replay_saves: ReplaySaveStateStore,
    /// Developer-only, in-memory switch that makes the live monitor include
    /// matcher regions and annotation sets in its debug/info payloads. This is
    /// intentionally not part of persisted settings.
    pub monitor_annotations_enabled: AtomicBool,
    /// Developer-only, transient (not persisted) standalone frame dump: captures a
    /// chosen source's frames to a temp directory independent of the monitor. See
    /// `routes::monitor::start_frame_dump`.
    pub frame_dump: std::sync::Mutex<Option<routes::monitor::FrameDumpHandle>>,
    /// Signals when OBS has emitted `OBS_FRONTEND_EVENT_FINISHED_LOADING` and
    /// frontend replay-buffer APIs are safe to query.
    pub frontend_ready_tx: watch::Sender<bool>,
    /// SQLite-backed index of saved run clips.
    pub run_catalog: std::sync::Arc<crate::db::run_catalog::RunCatalog>,
    /// Whether a new catalog needs its first clip import. The mutex prevents
    /// concurrent Runs requests from observing a partially seeded catalog.
    pub run_catalog_needs_seed: std::sync::Mutex<bool>,
    /// Plugin-owned user settings, loaded from and persisted to JSON.
    pub settings: crate::settings::SettingsStore,
    /// `Some(start instant)` if this core load followed a successful update apply
    /// (see `crate::WAS_RELOADED`), so a client connecting within a grace period
    /// gets a one-off "plugin updated" notice (see `routes::monitor::handle_socket`).
    pub reloaded_at: Option<std::time::Instant>,
}

pub struct PendingOAuth {
    pub state: String,
    pub tx: oneshot::Sender<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct MonitorSnapshot {
    pub enabled: bool,
    #[serde(rename = "sourceName", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_name: Option<String>,
    #[serde(rename = "cvLanguage", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "\"en\" | \"jp\"")]
    pub cv_language: Option<String>,
    pub wall_clocks: MonitorWallClockState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct MonitorWallClockState {
    #[ts(type = "number | null")]
    pub session_started_at_unix_ms: Option<u64>,
    #[ts(type = "number")]
    pub session_elapsed_ms: u64,
    pub session_running: bool,
    #[ts(type = "number | null")]
    pub level_started_at_unix_ms: Option<u64>,
    #[ts(type = "number")]
    pub level_elapsed_ms: u64,
    pub level_running: bool,
    pub level_paused: bool,
    pub level_start_reason: Option<LevelTimerStartReason>,
    pub level_timer_phase: LevelTimerPhase,
    #[ts(type = "number | null")]
    pub intro_swirl_delay_ms: Option<u64>,
    pub fade_detection: Option<BlackFrameSignal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub monitor: MonitorSnapshot,
    #[serde(rename = "match")]
    pub level_match: Option<LevelMatch>,
    pub run_catalog_sync: Option<RunCatalogSync>,
    pub recording_state: Option<RecordingStatus>,
    pub replay_saves: Vec<ReplaySaveStatus>,
    pub sources: Vec<routes::sources::Source>,
    pub replay_buffer: ReplayBufferStatus,
    pub settings_status: crate::settings::SettingsStatus,
    pub update: crate::updates::UpdateStatus,
}

#[derive(Clone)]
pub struct SharedStateStore {
    tx: watch::Sender<AppSnapshot>,
    state: Arc<StdMutex<AppSnapshot>>,
}

impl SharedStateStore {
    pub fn new(initial: AppSnapshot) -> Self {
        let (tx, _) = watch::channel(initial.clone());
        Self { tx, state: Arc::new(StdMutex::new(initial)) }
    }

    pub fn subscribe(&self) -> watch::Receiver<AppSnapshot> {
        self.tx.subscribe()
    }

    #[cfg(test)]
    pub fn current(&self) -> AppSnapshot {
        self.lock_state().clone()
    }

    pub fn set_monitor_running(&self, source_name: String, cv_language: String, clocks: MonitorWallClockState) {
        self.update(|state| {
            state.monitor.enabled = true;
            state.monitor.source_name = Some(source_name);
            state.monitor.cv_language.get_or_insert(cv_language);
            state.monitor.wall_clocks = clocks;
        });
    }

    pub fn set_monitor_language(&self, cv_language: String) {
        self.update(|state| state.monitor.cv_language = Some(cv_language));
    }

    pub fn set_monitor_stopped(&self, clocks: MonitorWallClockState) {
        self.update(|state| {
            state.monitor.wall_clocks = clocks;
            state.monitor.enabled = false;
            state.monitor.source_name = None;
            state.monitor.cv_language = None;
            state.level_match = None;
            state.recording_state = None;
        });
    }

    pub fn set_match(&self, level_match: Option<LevelMatch>, clocks: MonitorWallClockState) {
        self.update(|state| {
            state.level_match = level_match;
            state.monitor.wall_clocks = clocks;
        });
    }

    pub fn set_monitor_wall_clocks(&self, clocks: MonitorWallClockState) {
        self.update(|state| state.monitor.wall_clocks = clocks);
    }

    pub fn set_run_catalog_sync(&self, run_catalog_sync: Option<RunCatalogSync>) {
        self.update(|state| state.run_catalog_sync = run_catalog_sync);
    }

    pub fn set_recording_state(&self, recording_state: Option<RecordingStatus>) {
        self.update(|state| state.recording_state = recording_state);
    }

    pub fn set_replay_save(&self, replay_save: ReplaySaveStatus) {
        self.update(|state| {
            if let Some(existing) =
                state.replay_saves.iter_mut().find(|existing| existing.tracking_id == replay_save.tracking_id)
            {
                *existing = replay_save;
            } else {
                state.replay_saves.push(replay_save);
                state.replay_saves.sort_unstable_by_key(|save| std::cmp::Reverse(save.tracking_id));
            }
        });
    }

    pub fn update_replay_save(&self, tracking_id: u64, stage: ReplaySaveStage, error: Option<String>) {
        self.update(|state| {
            if let Some(existing) = state.replay_saves.iter_mut().find(|existing| existing.tracking_id == tracking_id) {
                existing.stage = stage;
                existing.error = error;
            }
        });
    }

    pub fn remove_replay_save(&self, tracking_id: u64) {
        self.update(|state| state.replay_saves.retain(|save| save.tracking_id != tracking_id));
    }

    pub fn set_sources(&self, sources: Vec<routes::sources::Source>) {
        self.update(|state| state.sources = sources);
    }

    pub fn set_replay_buffer(&self, replay_buffer: ReplayBufferStatus) {
        self.update(|state| state.replay_buffer = replay_buffer);
    }

    pub fn set_settings_status(&self, settings_status: crate::settings::SettingsStatus) {
        self.update(|state| state.settings_status = settings_status);
    }

    pub fn current_update_status(&self) -> crate::updates::UpdateStatus {
        self.lock_state().update.clone()
    }

    pub fn set_update_status(&self, update: crate::updates::UpdateStatus) {
        self.update(|state| state.update = update);
    }

    fn update(&self, apply: impl FnOnce(&mut AppSnapshot)) {
        let next = {
            let mut state = self.lock_state();
            let previous = state.clone();
            apply(&mut state);
            if *state == previous {
                return;
            }
            state.clone()
        };
        self.tx.send_replace(next);
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, AppSnapshot> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum RunCatalogSync {
    Initial,
    Manual,
}

/// A Discord webhook message we posted and may later edit.
pub struct StreamMessage {
    pub id: String,
    pub broadcast_url: String,
    pub webhook_url: String,
}

/// Messages pushed to app event-stream clients, internally tagged by `type`.
/// Retained state is carried by `Snapshot`; the other variants are one-off
/// events sent only to connected clients.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum AppEvent {
    /// Sent once on connect: the build id of the SPA this backend serves. The
    /// SPA compares it against its own served build and reloads on mismatch, so
    /// a stale tab picks up the new frontend. See [`routes::index::BUILD_ID`].
    Version {
        #[serde(rename = "buildId")]
        build_id: String,
    },
    /// The complete retained app/session state. Sent on connect and after every
    /// retained-state change so new tabs sync to the backend source of truth.
    Snapshot { state: Box<AppSnapshot> },
    /// Rolling monitor throughput and backend-owned matcher health. Captured
    /// frames are either processed or superseded in the latest-frame mailbox.
    MonitorFps(MonitorFps),
    /// A run's clip save was scheduled and will fire after the post-run padding.
    RecordingSavePending(RecordingSavePending),
    /// A run's clip was saved out of the replay buffer and trimmed.
    RecordingSaved(RecordingSaved),
    RunCatalogChanged {
        #[serde(rename = "runId", skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        run_id: Option<String>,
        #[serde(rename = "saveId", skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "number")]
        save_id: Option<u64>,
    },
    /// Monitoring stopped, either from a user request or an external OBS event.
    MonitorStopped { reason: MonitorStoppedReason },
    /// The settings JSON file changed on disk and was reloaded successfully.
    SettingsReloaded {
        #[serde(rename = "configPath")]
        config_path: String,
        #[ts(type = "AppSettings")]
        settings: crate::settings::AppSettings,
    },
    /// The settings JSON file changed on disk but could not be parsed or read.
    SettingsInvalid {
        #[serde(rename = "configPath")]
        config_path: String,
        error: String,
    },
    /// Sent once when a client connects shortly after this core was loaded via
    /// an applied update (dev hot-reload or a real auto-update), so the SPA
    /// can show a one-off "plugin updated" notice. See `AppStateInner::reloaded_at`.
    UpdateApplied {
        version: String,
        /// GitHub release page for `version`, but only when the persisted
        /// `last_known_update_version` matches the running version (i.e. this is
        /// the update just applied). `None` otherwise, to avoid a wrong link.
        #[serde(rename = "releaseUrl", skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        release_url: Option<String>,
    },
    /// A newer release was found but downloading/verifying/staging it failed
    /// (e.g. an unwritable install directory), so no update is queued up to
    /// apply. One-off, delivered via `event_tx` -- see `updates::check_for_updates_now`.
    UpdateStagingFailed { error: String },
    /// A YouTube upload was queued, progressed, completed, or failed.
    YoutubeUploadChanged { upload: crate::youtube::YoutubeUploadStatus },
    /// YouTube connection state changed in another browser client.
    YoutubeStatusChanged { status: crate::youtube::YoutubeStatus },
}

/// Why the backend stopped an active monitor. Serialized as a plain string
/// inside [`AppEvent::MonitorStopped`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum MonitorStoppedReason {
    /// A client requested `/api/v1/monitor/stop`.
    UserStopped,
    /// OBS reported that its replay buffer stopped while monitoring was active.
    ReplayBufferStopped,
}

/// Rolling capture/processing throughput pushed while monitoring is active.
#[derive(Debug, Clone, Copy, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct MonitorFps {
    pub processed_fps: f64,
    pub captured_fps: f64,
    pub source_fps: f64,
    #[ts(type = "number")]
    pub dropped_frames: u64,
    pub health: MonitorFpsHealth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum MonitorFpsHealth {
    Healthy,
    Warning,
    Lagging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum ReplaySaveStage {
    Scheduled,
    WaitingForReplaySave,
    SavingReplay,
    Trimming,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ReplaySaveStatus {
    #[ts(type = "number")]
    pub tracking_id: u64,
    #[ts(type = "number")]
    pub save_id: u64,
    pub stage: ReplaySaveStage,
    pub level: String,
    pub difficulty: Option<String>,
    pub run_status: String,
    pub estimated_duration_secs: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct ReplaySaveStateStore {
    snapshot: SharedStateStore,
}

impl ReplaySaveStateStore {
    const COMPLETED_LINGER: Duration = Duration::from_secs(5);
    const FAILED_LINGER: Duration = Duration::from_secs(30);

    pub fn new(snapshot: SharedStateStore) -> Self {
        Self { snapshot }
    }

    pub fn schedule(&self, status: ReplaySaveStatus) {
        self.snapshot.set_replay_save(status);
    }

    #[cfg(test)]
    pub fn current(&self) -> Vec<ReplaySaveStatus> {
        self.snapshot.current().replay_saves
    }

    pub fn transition(&self, tracking_id: u64, stage: ReplaySaveStage) {
        self.snapshot.update_replay_save(tracking_id, stage, None);
    }

    pub fn complete(&self, tracking_id: u64) {
        self.snapshot.update_replay_save(tracking_id, ReplaySaveStage::Completed, None);
        self.remove_after(tracking_id, Self::COMPLETED_LINGER);
    }

    pub fn fail(&self, tracking_id: u64, error: String) {
        self.snapshot.update_replay_save(tracking_id, ReplaySaveStage::Failed, Some(error));
        self.remove_after(tracking_id, Self::FAILED_LINGER);
    }

    fn remove_after(&self, tracking_id: u64, duration: Duration) {
        let store = self.clone();
        let spawned = std::thread::Builder::new().name("ge-replay-save-state-timeout".to_owned()).spawn(move || {
            std::thread::sleep(duration);
            store.snapshot.remove_replay_save(tracking_id);
        });
        if let Err(err) = spawned {
            tracing::error!("failed to spawn replay save state timeout thread: {err}");
        }
    }
}

/// Details of a clip save that has been scheduled after a run ending was seen,
/// pushed to clients as an [`AppEvent::RecordingSavePending`].
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RecordingSavePending {
    /// Identifier shared with the matching [`RecordingSaved`] event.
    #[ts(type = "number")]
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
    #[ts(optional)]
    pub level_number: Option<i32>,
    /// Human-readable difficulty label, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub difficulty: Option<String>,
    /// Run time read from the stats screen, in seconds, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub time_secs: Option<i32>,
    /// Target time read from the stats screen, in seconds, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub target_time_secs: Option<i32>,
    /// Best time read from the stats screen, in seconds, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub best_time_secs: Option<i32>,
    /// The stats-screen match the clip will be named from, when one was seen.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stats: Option<LevelMatch>,
}

/// Details of a clip saved out of the replay buffer at the end of a run, pushed
/// to clients as an [`AppEvent::RecordingSaved`].
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RecordingSaved {
    /// Identifier shared with the matching [`RecordingSavePending`] event.
    #[ts(type = "number")]
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
    #[ts(optional)]
    pub stats: Option<LevelMatch>,
}

pub type AppState = Arc<AppStateInner>;

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
