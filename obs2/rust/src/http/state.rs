use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::{Mutex, broadcast, oneshot, watch};

use super::{ReplayBufferStatus, routes};
use crate::cv::LevelMatch;

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
    pub monitor: std::sync::Mutex<Option<routes::monitor::MonitorHandle>>,
    /// The single retained app/session state object. New browser clients receive
    /// this on connect, then every retained-state change as a fresh snapshot.
    pub snapshot: SharedStateStore,
    /// One-off app events broadcast to connected clients (e.g. a clip being
    /// saved). Discrete events are not retained for late joiners.
    pub event_tx: broadcast::Sender<AppEvent>,
    /// Latest recorder phase from the running monitor, with generation-aware
    /// timeout clearing. Writes also update `snapshot.recording_state`.
    pub recording_state: RecordingStateStore,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSnapshot {
    pub enabled: bool,
    #[serde(rename = "sourceName", skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub monitor: MonitorSnapshot,
    #[serde(rename = "match")]
    pub level_match: Option<LevelMatch>,
    pub recording_state: Option<RecordingStatus>,
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

    pub fn set_monitor_running(&self, source_name: String) {
        self.update(|state| {
            state.monitor.enabled = true;
            state.monitor.source_name = Some(source_name);
        });
    }

    pub fn set_monitor_stopped(&self) {
        self.update(|state| {
            state.monitor.enabled = false;
            state.monitor.source_name = None;
            state.level_match = None;
            state.recording_state = None;
        });
    }

    pub fn set_match(&self, level_match: Option<LevelMatch>) {
        self.update(|state| state.level_match = level_match);
    }

    pub fn set_recording_state(&self, recording_state: Option<RecordingStatus>) {
        self.update(|state| state.recording_state = recording_state);
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

/// A Discord webhook message we posted and may later edit.
pub struct StreamMessage {
    pub id: String,
    pub broadcast_url: String,
    pub webhook_url: String,
}

/// Messages pushed to app event-stream clients, internally tagged by `type`.
/// Retained state is carried by `Snapshot`; the other variants are one-off
/// events sent only to connected clients.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
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
    /// A scheduled save was dropped before any clip was written (e.g. a failed
    /// run shorter than the configured minimum), so no `RecordingSaved` follows.
    /// Clients use it to clear the pending "saving" notification for this save.
    RecordingSaveDiscarded {
        #[serde(rename = "saveId")]
        save_id: u64,
    },
    /// A failed run reached an ending screen but no clip was written for it
    /// (failed-run saving is disabled, or the run was shorter than the
    /// configured minimum). Unlike a recording-phase transition this is a
    /// one-off notification that never touches the retained recorder phase, so a
    /// discard that fires late -- e.g. on the save timer, after a new run has
    /// already started -- can't knock the new run out of its "recording" state.
    FailedRunNotSaved { reason: FailedRunNotSavedReason },
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
    /// A YouTube upload was queued, progressed, completed, or failed.
    YoutubeUploadChanged { upload: crate::youtube::YoutubeUploadStatus },
}

/// Why the backend stopped an active monitor. Serialized as a plain string
/// inside [`AppEvent::MonitorStopped`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MonitorStoppedReason {
    /// A client requested `/api/v1/monitor/stop`.
    UserStopped,
    /// OBS reported that its replay buffer stopped while monitoring was active.
    ReplayBufferStopped,
}

/// Why a failed run reached an ending screen without a clip being written.
/// Serialized as a plain string inside [`AppEvent::FailedRunNotSaved`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FailedRunNotSavedReason {
    /// Failed-run saving is disabled in the active recording options.
    SavingDisabled,
    /// The run was shorter than the configured minimum failed-run length.
    TooShort,
}

/// Monitor throughput sampled by the worker thread and pushed to the frontend
/// while monitoring is active.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorFps {
    pub processed_fps: f64,
    pub source_fps: f64,
}

/// A transition in the recorder's per-run state, retained in [`AppSnapshot`] so
/// the SPA can reflect where a run is in its lifecycle. Serialized as a plain
/// string, e.g. `"started"`.
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
    /// [`AppEvent::RecordingSaved`] follows. (A failed run does this normally.)
    StatsSkipped,
    /// A run ended at the stats screen (or, via `StatsSkipped`, the report
    /// screen): a save has been scheduled and will fire a few seconds later. A
    /// [`AppEvent::RecordingSaved`] follows once the clip is written.
    SavePending,
}

/// Retained recorder phase shared by the monitor worker and app snapshot.
/// Transient phases are cleared here so the backend owns the same lifecycle the
/// UI displays.
#[derive(Clone)]
pub struct RecordingStateStore {
    snapshot: SharedStateStore,
    state: Arc<StdMutex<RecordingStateInner>>,
}

struct RecordingStateInner {
    status: Option<RecordingStatus>,
    generation: u64,
}

impl RecordingStateStore {
    const CANCELLED_LINGER: Duration = Duration::from_secs(2);
    const SAVE_TIMEOUT: Duration = Duration::from_secs(30);

    pub fn new(snapshot: SharedStateStore) -> Self {
        RecordingStateStore {
            snapshot,
            state: Arc::new(StdMutex::new(RecordingStateInner { status: None, generation: 0 })),
        }
    }

    pub fn current(&self) -> Option<RecordingStatus> {
        self.lock_state().status
    }

    /// Set the retained phase, returning the generation this write landed on.
    /// Pass it to [`Self::clear_if_generation`] to later clear *this* transition
    /// specifically, rather than whatever the phase happens to be by then.
    pub fn set(&self, status: RecordingStatus) -> u64 {
        let generation = {
            let mut state = self.lock_state();
            let previous = state.status;
            state.generation += 1;
            state.status = Some(status);
            self.snapshot.set_recording_state(state.status);
            tracing::info!(?previous, new = ?status, generation = state.generation, "recording phase set");
            state.generation
        };

        match status {
            RecordingStatus::Cancelled => {
                self.clear_after(generation, Self::CANCELLED_LINGER);
            }
            RecordingStatus::SavePending | RecordingStatus::StatsSkipped => {
                self.clear_after(generation, Self::SAVE_TIMEOUT);
            }
            _ => {}
        }

        generation
    }

    pub fn clear(&self) {
        let mut state = self.lock_state();
        let previous = state.status;
        state.generation += 1;
        state.status = None;
        self.snapshot.set_recording_state(state.status);
        tracing::info!(?previous, generation = state.generation, "recording phase cleared");
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

    /// Clear the phase only if it's still on transition `generation` -- i.e.
    /// nothing has `set`/`clear`'d it since. Stops a slow async save from
    /// clearing a newer, unrelated run's phase that happens to hold the same
    /// status value (e.g. two runs both showing `SavePending`).
    pub fn clear_if_generation(&self, generation: u64) {
        let mut state = self.lock_state();
        if state.generation == generation {
            let previous = state.status;
            state.generation += 1;
            state.status = None;
            self.snapshot.set_recording_state(state.status);
            tracing::info!(
                ?previous,
                cleared_generation = generation,
                "recording phase cleared (timed out / save done)"
            );
        }
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, RecordingStateInner> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Details of a clip save that has been scheduled after a run ending was seen,
/// pushed to clients as an [`AppEvent::RecordingSavePending`].
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
/// to clients as an [`AppEvent::RecordingSaved`].
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

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
