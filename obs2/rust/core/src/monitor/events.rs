//! Explicit, synchronous routing from monitor observations to clocks and publication.

use std::time::{SystemTime, UNIX_EPOCH};

use super::clocks::MonitorClocks;
use crate::cv::{BlackFrameSignal, LevelMatch, WatchTransition};
use crate::http::SharedStateStore;

pub(super) enum MonitorEvent {
    SessionStarted { source_name: String, language: String },
    MatchObserved(LevelMatch),
    BlackFrameObserved(BlackFrameSignal),
    WatchChanged(WatchTransition),
    SessionStopped,
}

/// Owned only by the monitor worker. Each event updates clocks before publishing its snapshot.
pub(super) struct MonitorEvents {
    clocks: MonitorClocks,
    snapshot: SharedStateStore,
}

impl MonitorEvents {
    pub(super) fn new(snapshot: SharedStateStore) -> Self {
        Self { clocks: MonitorClocks::default(), snapshot }
    }

    pub(super) fn handle(&mut self, event: MonitorEvent, at_ms: u64) {
        match event {
            MonitorEvent::SessionStarted { source_name, language } => {
                self.clocks.start_session(at_ms);
                self.snapshot.set_monitor_running(source_name, language, self.clocks.snapshot());
            }
            MonitorEvent::SessionStopped => {
                if self.clocks.running() {
                    self.clocks.stop_session(at_ms);
                    self.snapshot.set_monitor_stopped(self.clocks.snapshot());
                }
            }
            _ if !self.clocks.running() => {}
            MonitorEvent::MatchObserved(level_match) => {
                self.clocks.reconcile_match(&level_match, at_ms);
                self.snapshot.set_match(Some(level_match), self.clocks.snapshot());
            }
            MonitorEvent::BlackFrameObserved(signal) => {
                if self.clocks.reconcile_black_frame(signal, at_ms) {
                    self.snapshot.set_monitor_wall_clocks(self.clocks.snapshot());
                }
            }
            MonitorEvent::WatchChanged(transition) => {
                if self.clocks.reconcile_watch_transition(transition, at_ms) {
                    self.snapshot.set_monitor_wall_clocks(self.clocks.snapshot());
                }
            }
        }
    }
}

impl Drop for MonitorEvents {
    fn drop(&mut self) {
        // The worker drops recording before this owner, including during unwinding.
        self.handle(MonitorEvent::SessionStopped, unix_time_ms());
    }
}

pub(super) fn unix_time_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "events_test.rs"]
mod tests;
