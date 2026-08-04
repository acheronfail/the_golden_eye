use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::{Mutex, broadcast, oneshot, watch};

use super::{ReplayBufferStatus, routes};
use crate::cv::{BlackFrameSignal, LevelMatch, WatchTransition};

const FADE_DIAGNOSTICS_INTERVAL_MS: u64 = 250;
const END_FADE_CONFIRMATION_MS: u64 = 250;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSnapshot {
    pub enabled: bool,
    #[serde(rename = "sourceName", skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
    #[serde(rename = "cvLanguage", skip_serializing_if = "Option::is_none")]
    pub cv_language: Option<String>,
    pub wall_clocks: MonitorWallClockState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorWallClockState {
    pub session_started_at_unix_ms: Option<u64>,
    pub session_elapsed_ms: u64,
    pub session_running: bool,
    pub level_started_at_unix_ms: Option<u64>,
    pub level_elapsed_ms: u64,
    pub level_running: bool,
    pub level_paused: bool,
    pub level_start_reason: Option<LevelTimerStartReason>,
    pub level_timer_phase: LevelTimerPhase,
    pub intro_swirl_delay_ms: Option<u64>,
    pub fade_detection: Option<BlackFrameSignal>,
    #[serde(skip)]
    second_cutscene_started_at_ms: Option<u64>,
    #[serde(skip)]
    second_cutscene_visible: bool,
    #[serde(skip)]
    black_frame_active: bool,
    #[serde(skip)]
    end_fade_started_at_ms: Option<u64>,
    #[serde(skip)]
    fade_diagnostics_published_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LevelTimerStartReason {
    Fade,
    Swirl,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LevelTimerPhase {
    #[default]
    Idle,
    AwaitingInitialBlack,
    AwaitingFirstCutscene,
    AwaitingFirstCutsceneFade,
    AwaitingSecondFadeOrSwirl,
    AwaitingGameplayAfterSkip,
    Running,
    Stopped,
}

impl MonitorWallClockState {
    fn start_session(&mut self, now_ms: u64) {
        *self = Self { session_started_at_unix_ms: Some(now_ms), session_running: true, ..Self::default() };
    }

    fn stop_session(&mut self, now_ms: u64) {
        self.session_elapsed_ms = elapsed_ms(self.session_started_at_unix_ms, self.session_elapsed_ms, now_ms);
        self.session_started_at_unix_ms = None;
        self.session_running = false;
        self.stop_level(now_ms);
    }

    fn reconcile_screen(&mut self, screen: crate::cv::Screen, now_ms: u64) {
        match screen {
            screen if screen.is_level_launch() => {
                self.level_started_at_unix_ms = None;
                self.level_elapsed_ms = 0;
                self.level_running = false;
                self.level_paused = false;
                self.level_start_reason = None;
                self.level_timer_phase = LevelTimerPhase::AwaitingInitialBlack;
                self.intro_swirl_delay_ms = None;
                self.fade_detection = None;
                self.second_cutscene_started_at_ms = None;
                self.second_cutscene_visible = false;
                self.black_frame_active = false;
                self.end_fade_started_at_ms = None;
                self.fade_diagnostics_published_at_ms = None;
            }
            crate::cv::Screen::Unknown => {}
            _ => {
                self.stop_level(now_ms);
            }
        }
    }

    fn reconcile_match(&mut self, level_match: &LevelMatch, now_ms: u64) {
        self.reconcile_screen(level_match.screen, now_ms);
        if level_match.screen.is_level_launch() {
            self.intro_swirl_delay_ms = crate::ge::Level::from_matcher(level_match.mission, level_match.part)
                .map(crate::ge::intro::swirl_delay_ms);
        }
    }

    fn reconcile_black_frame(&mut self, signal: BlackFrameSignal, now_ms: u64) -> bool {
        let classification_changed = signal.detected != self.black_frame_active;
        if classification_changed {
            self.black_frame_active = signal.detected;
        }
        let timer_changed = self.reconcile_level_timer(signal.detected, classification_changed, now_ms);
        let region_changed =
            self.fade_detection.as_ref().map(|current| current.sample_region) != Some(signal.sample_region);
        let diagnostics_changed = self.fade_detection != Some(signal);
        let diagnostics_due = self
            .fade_diagnostics_published_at_ms
            .is_none_or(|published_at| now_ms.saturating_sub(published_at) >= FADE_DIAGNOSTICS_INTERVAL_MS);
        if !timer_changed && !classification_changed && !region_changed && !(diagnostics_changed && diagnostics_due) {
            return false;
        }
        self.fade_detection = Some(signal);
        self.fade_diagnostics_published_at_ms = Some(now_ms);
        true
    }

    fn reconcile_level_timer(&mut self, black: bool, edge: bool, now_ms: u64) -> bool {
        let previous_phase = self.level_timer_phase;
        if let Some(deadline) = self.pending_swirl_deadline()
            && now_ms >= deadline
        {
            self.start_level(deadline, LevelTimerStartReason::Swirl);
            if edge && black {
                self.end_fade_started_at_ms = Some(now_ms);
            }
            return self.level_timer_phase != previous_phase;
        }

        if self.level_timer_phase == LevelTimerPhase::Running {
            if self.level_paused {
                self.end_fade_started_at_ms = None;
                return self.level_timer_phase != previous_phase;
            }
            if edge {
                self.end_fade_started_at_ms = black.then_some(now_ms);
            }
            if black
                && let Some(started_at) = self.end_fade_started_at_ms
                && now_ms.saturating_sub(started_at) >= END_FADE_CONFIRMATION_MS
            {
                self.stop_level(started_at);
            }
            return self.level_timer_phase != previous_phase;
        }

        if edge {
            match (self.level_timer_phase, black) {
                (LevelTimerPhase::AwaitingInitialBlack, true) => {
                    self.level_timer_phase = LevelTimerPhase::AwaitingFirstCutscene;
                }
                (LevelTimerPhase::AwaitingFirstCutscene, false) => {
                    self.level_timer_phase = LevelTimerPhase::AwaitingFirstCutsceneFade;
                }
                (LevelTimerPhase::AwaitingFirstCutsceneFade, true) => {
                    self.level_timer_phase = LevelTimerPhase::AwaitingSecondFadeOrSwirl;
                    self.second_cutscene_started_at_ms = None;
                    self.second_cutscene_visible = false;
                }
                (LevelTimerPhase::AwaitingSecondFadeOrSwirl, false) => {
                    self.second_cutscene_started_at_ms = Some(now_ms);
                    self.second_cutscene_visible = true;
                }
                (LevelTimerPhase::AwaitingSecondFadeOrSwirl, true) if self.second_cutscene_visible => {
                    self.level_timer_phase = LevelTimerPhase::AwaitingGameplayAfterSkip;
                }
                (LevelTimerPhase::AwaitingGameplayAfterSkip, false) => {
                    self.start_level_with_elapsed(
                        now_ms,
                        crate::ge::intro::SKIPPED_SWIRL_INITIAL_ELAPSED_MS,
                        LevelTimerStartReason::Fade,
                    );
                }
                _ => {}
            }
        }
        self.level_timer_phase != previous_phase
    }

    fn pending_swirl_deadline(&self) -> Option<u64> {
        if self.level_timer_phase != LevelTimerPhase::AwaitingSecondFadeOrSwirl {
            return None;
        }
        let started_at = self.second_cutscene_started_at_ms?;
        let delay = self.intro_swirl_delay_ms?;
        Some(started_at.saturating_add(delay))
    }

    fn start_level(&mut self, now_ms: u64, reason: LevelTimerStartReason) {
        self.start_level_with_elapsed(now_ms, 0, reason);
    }

    fn start_level_with_elapsed(&mut self, now_ms: u64, elapsed_ms: u64, reason: LevelTimerStartReason) {
        self.level_started_at_unix_ms = Some(now_ms.saturating_sub(elapsed_ms));
        self.level_elapsed_ms = elapsed_ms;
        self.level_running = true;
        self.level_paused = false;
        self.level_start_reason = Some(reason);
        self.level_timer_phase = LevelTimerPhase::Running;
        self.second_cutscene_started_at_ms = None;
        self.second_cutscene_visible = false;
        self.end_fade_started_at_ms = None;
    }

    fn stop_level(&mut self, now_ms: u64) {
        self.level_elapsed_ms = elapsed_ms(self.level_started_at_unix_ms, self.level_elapsed_ms, now_ms);
        self.level_started_at_unix_ms = None;
        self.level_running = false;
        self.level_paused = false;
        self.level_timer_phase = LevelTimerPhase::Stopped;
        self.end_fade_started_at_ms = None;
    }

    fn reconcile_watch_transition(&mut self, transition: WatchTransition, now_ms: u64) -> bool {
        if self.level_timer_phase != LevelTimerPhase::Running {
            return false;
        }
        match transition {
            WatchTransition::Paused if !self.level_paused => {
                self.level_elapsed_ms = elapsed_ms(self.level_started_at_unix_ms, self.level_elapsed_ms, now_ms);
                self.level_started_at_unix_ms = None;
                self.level_running = false;
                self.level_paused = true;
                self.end_fade_started_at_ms = None;
                true
            }
            WatchTransition::Resumed if self.level_paused => {
                self.level_started_at_unix_ms = Some(now_ms.saturating_sub(self.level_elapsed_ms));
                self.level_running = true;
                self.level_paused = false;
                true
            }
            _ => false,
        }
    }
}

fn elapsed_ms(started_at_ms: Option<u64>, frozen_ms: u64, now_ms: u64) -> u64 {
    started_at_ms.map_or(frozen_ms, |started_at_ms| now_ms.saturating_sub(started_at_ms))
}

fn unix_time_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().try_into().unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
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

    pub fn set_monitor_running(&self, source_name: String, cv_language: String) {
        let now_ms = unix_time_ms();
        self.update(|state| {
            state.monitor.enabled = true;
            state.monitor.source_name = Some(source_name);
            state.monitor.cv_language.get_or_insert(cv_language);
            state.monitor.wall_clocks.start_session(now_ms);
            if let Some(level_match) = state.level_match.as_ref() {
                state.monitor.wall_clocks.reconcile_match(level_match, now_ms);
            }
        });
    }

    pub fn set_monitor_language(&self, cv_language: String) {
        self.update(|state| state.monitor.cv_language = Some(cv_language));
    }

    pub fn set_monitor_stopped(&self) {
        let now_ms = unix_time_ms();
        self.update(|state| {
            state.monitor.wall_clocks.stop_session(now_ms);
            state.monitor.enabled = false;
            state.monitor.source_name = None;
            state.monitor.cv_language = None;
            state.level_match = None;
            state.recording_state = None;
        });
    }

    pub fn set_match(&self, level_match: Option<LevelMatch>) {
        let now_ms = unix_time_ms();
        self.update(|state| {
            if state.monitor.enabled
                && let Some(level_match) = level_match.as_ref()
            {
                state.monitor.wall_clocks.reconcile_match(level_match, now_ms);
            }
            state.level_match = level_match;
        });
    }

    pub fn observe_black_frame(&self, signal: BlackFrameSignal) {
        let now_ms = unix_time_ms();
        let next = {
            let mut state = self.lock_state();
            if !state.monitor.enabled || !state.monitor.wall_clocks.reconcile_black_frame(signal, now_ms) {
                return;
            }
            state.clone()
        };
        self.tx.send_replace(next);
    }

    pub fn observe_watch_transition(&self, transition: WatchTransition, observed_at_unix_ms: u64) {
        let next = {
            let mut state = self.lock_state();
            if !state.monitor.enabled
                || !state.monitor.wall_clocks.reconcile_watch_transition(transition, observed_at_unix_ms)
            {
                return;
            }
            state.clone()
        };
        self.tx.send_replace(next);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
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
    /// Rolling monitor throughput and backend-owned matcher health. Captured
    /// frames are either processed or superseded in the latest-frame mailbox.
    MonitorFps(MonitorFps),
    /// A run's clip save was scheduled and will fire after the post-run padding.
    RecordingSavePending(RecordingSavePending),
    /// A run's clip was saved out of the replay buffer and trimmed.
    RecordingSaved(RecordingSaved),
    RunCatalogChanged {
        #[serde(rename = "runId", skip_serializing_if = "Option::is_none")]
        run_id: Option<String>,
        #[serde(rename = "saveId", skip_serializing_if = "Option::is_none")]
        save_id: Option<u64>,
    },
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
    /// YouTube connection state changed in another browser client.
    YoutubeStatusChanged { status: crate::youtube::YoutubeStatus },
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

/// Rolling capture/processing throughput pushed while monitoring is active.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorFps {
    pub processed_fps: f64,
    pub captured_fps: f64,
    pub source_fps: f64,
    pub dropped_frames: u64,
    pub health: MonitorFpsHealth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MonitorFpsHealth {
    Healthy,
    Warning,
    Lagging,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReplaySaveStage {
    Scheduled,
    WaitingForReplaySave,
    SavingReplay,
    Trimming,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaySaveStatus {
    pub tracking_id: u64,
    pub save_id: u64,
    pub stage: ReplaySaveStage,
    pub level: String,
    pub difficulty: Option<String>,
    pub run_status: String,
    pub estimated_duration_secs: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
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
            RecordingStatus::Started
            | RecordingStatus::Failed
            | RecordingStatus::Aborted
            | RecordingStatus::Kia
            | RecordingStatus::Complete => {}
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
