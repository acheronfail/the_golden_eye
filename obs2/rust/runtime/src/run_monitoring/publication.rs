//! Monitor display models and publication; timer and recording rules live with their owners.
use std::time::Duration;

use ge_cv::{BlackFrameSignal, LevelMatch};
use serde::Serialize;

pub use super::in_game_timer::{LevelTimerPhase, LevelTimerStartReason};
use crate::app::SharedStateStore;

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

#[derive(Clone)]
pub(crate) struct MonitorPublisher {
    snapshot: SharedStateStore,
}

impl MonitorPublisher {
    pub(crate) fn new(snapshot: SharedStateStore) -> Self {
        Self { snapshot }
    }
    pub fn set_monitor_running(&self, source_name: String, cv_language: String, clocks: MonitorWallClockState) {
        self.snapshot.update(|state| {
            state.monitor.enabled = true;
            state.monitor.source_name = Some(source_name);
            state.monitor.cv_language.get_or_insert(cv_language);
            state.monitor.wall_clocks = clocks;
        });
    }

    pub fn set_monitor_language(&self, cv_language: String) {
        self.snapshot.update(|state| state.monitor.cv_language = Some(cv_language));
    }

    pub fn set_monitor_stopped(&self, clocks: MonitorWallClockState) {
        self.snapshot.update(|state| {
            state.monitor.wall_clocks = clocks;
            state.monitor.enabled = false;
            state.monitor.source_name = None;
            state.monitor.cv_language = None;
            state.level_match = None;
            state.recording_state = None;
        });
    }

    pub fn set_match(&self, level_match: Option<LevelMatch>, clocks: MonitorWallClockState) {
        self.snapshot.update(|state| {
            state.level_match = level_match;
            state.monitor.wall_clocks = clocks;
        });
    }

    pub fn set_monitor_wall_clocks(&self, clocks: MonitorWallClockState) {
        self.snapshot.update(|state| state.monitor.wall_clocks = clocks);
    }
}

/// Why the backend stopped an active monitor. Serialized as a plain string
/// inside [`crate::app::AppEvent::MonitorStopped`].
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
        self.snapshot.update(|state| {
            if let Some(existing) =
                state.replay_saves.iter_mut().find(|existing| existing.tracking_id == status.tracking_id)
            {
                *existing = status;
            } else {
                state.replay_saves.push(status);
                state.replay_saves.sort_unstable_by_key(|save| std::cmp::Reverse(save.tracking_id));
            }
        });
    }

    #[cfg(test)]
    pub fn current(&self) -> Vec<ReplaySaveStatus> {
        self.snapshot.current().replay_saves
    }

    pub fn transition(&self, tracking_id: u64, stage: ReplaySaveStage) {
        self.update(tracking_id, stage, None);
    }

    pub fn complete(&self, tracking_id: u64) {
        self.update(tracking_id, ReplaySaveStage::Completed, None);
        self.remove_after(tracking_id, Self::COMPLETED_LINGER);
    }

    pub fn fail(&self, tracking_id: u64, error: String) {
        self.update(tracking_id, ReplaySaveStage::Failed, Some(error));
        self.remove_after(tracking_id, Self::FAILED_LINGER);
    }

    fn update(&self, tracking_id: u64, stage: ReplaySaveStage, error: Option<String>) {
        self.snapshot.update(|state| {
            if let Some(existing) = state.replay_saves.iter_mut().find(|existing| existing.tracking_id == tracking_id) {
                existing.stage = stage;
                existing.error = error;
            }
        });
    }

    fn remove_after(&self, tracking_id: u64, duration: Duration) {
        let store = self.clone();
        let spawned = std::thread::Builder::new().name("ge-replay-save-state-timeout".to_owned()).spawn(move || {
            std::thread::sleep(duration);
            store.snapshot.update(|state| state.replay_saves.retain(|save| save.tracking_id != tracking_id));
        });
        if let Err(err) = spawned {
            tracing::error!("failed to spawn replay save state timeout thread: {err}");
        }
    }
}

/// Details of a clip save that has been scheduled after a run ending was seen,
/// pushed to clients as an [`crate::app::AppEvent::RecordingSavePending`].
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
/// to clients as an [`crate::app::AppEvent::RecordingSaved`].
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
