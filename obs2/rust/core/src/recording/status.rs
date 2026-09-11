//! Owns the retained recording phase, expiry policy, and stale-completion checks.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde::Serialize;

use crate::http::SharedStateStore;

/// A transition in the recorder's per-run state, retained in [`crate::http::AppSnapshot`] so
/// the SPA can reflect where a run is in its lifecycle. Serialized as a plain
/// string, e.g. `"started"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
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
    /// [`crate::http::AppEvent::RecordingSaved`] follows. (A failed run does this normally.)
    StatsSkipped,
    /// A run ended at the stats screen (or, via `StatsSkipped`, the report
    /// screen): a save has been scheduled and will fire a few seconds later. A
    /// [`crate::http::AppEvent::RecordingSaved`] follows once the clip is written.
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

    /// Clear only this generation, preventing a late save or timeout from
    /// clearing a newer run's phase, even when both have the same status.
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

#[cfg(test)]
#[path = "tests/status.rs"]
mod tests;
