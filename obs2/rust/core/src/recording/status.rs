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

#[derive(Debug)]
pub(crate) enum RecordingStateEvent {
    PhaseChanged(RecordingStatus),
    Reset,
    SaveFinished(u64),
    Expired(u64),
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

    /// Apply and publish under one lock so concurrent completions cannot reorder snapshots.
    /// The returned generation identifies this phase for later completion or expiry.
    pub(crate) fn handle(&self, event: RecordingStateEvent) -> u64 {
        let (generation, expires_after) = {
            let mut state = self.lock_state();
            let next = match event {
                RecordingStateEvent::PhaseChanged(status) => Some(status),
                RecordingStateEvent::Reset => None,
                RecordingStateEvent::SaveFinished(generation) | RecordingStateEvent::Expired(generation) => {
                    if state.generation != generation {
                        return state.generation;
                    }
                    None
                }
            };
            let previous = state.status;
            state.generation += 1;
            state.status = next;
            self.snapshot.set_recording_state(next);
            tracing::info!(?event, ?previous, ?next, generation = state.generation, "recording phase updated");
            let expires_after = match next {
                Some(RecordingStatus::Cancelled) => Some(Self::CANCELLED_LINGER),
                Some(RecordingStatus::SavePending | RecordingStatus::StatsSkipped) => Some(Self::SAVE_TIMEOUT),
                _ => None,
            };
            (state.generation, expires_after)
        };
        if let Some(duration) = expires_after {
            let store = self.clone();
            let spawned = std::thread::Builder::new().name("ge-recording-state-timeout".to_owned()).spawn(move || {
                std::thread::sleep(duration);
                store.handle(RecordingStateEvent::Expired(generation));
            });
            if let Err(err) = spawned {
                tracing::error!("failed to spawn recording-state timeout thread: {err}");
            }
        }
        generation
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, RecordingStateInner> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
#[path = "tests/status.rs"]
mod tests;
