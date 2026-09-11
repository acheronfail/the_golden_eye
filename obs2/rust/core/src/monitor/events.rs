//! Routes monitor observations to recording, clocks, and retained display state.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::clocks::MonitorClocks;
use crate::cv::{BlackFrameSignal, LevelMatch, WatchTransition};
use crate::http::SharedStateStore;
use crate::recording::{RecordingEvent, RecordingState};

pub(super) enum MonitorEvent<'a> {
    SessionStarted { source_name: String, language: String },
    RawMatchObserved { matched: &'a LevelMatch, now: Instant },
    DisplayMatchObserved(LevelMatch),
    LanguageChanged(String),
    SaveDeadlineReached(Instant),
    BlackFrameObserved(BlackFrameSignal),
    WatchChanged(WatchTransition),
    SessionStopping,
    SessionStopped,
}

/// Owned only by the monitor worker; consumers run synchronously in routing order.
pub(super) struct MonitorEvents {
    clocks: MonitorClocks,
    recording: Option<RecordingState>,
    snapshot: SharedStateStore,
}

impl MonitorEvents {
    pub(super) fn new(snapshot: SharedStateStore, recording: RecordingState) -> Self {
        Self { clocks: MonitorClocks::default(), recording: Some(recording), snapshot }
    }

    pub(super) fn pending_fire_at(&self) -> Option<Instant> {
        self.recording.as_ref().and_then(RecordingState::pending_fire_at)
    }

    pub(super) fn handle(&mut self, event: MonitorEvent<'_>, at_ms: u64) {
        match event {
            MonitorEvent::SessionStarted { source_name, language } => {
                self.clocks.start_session(at_ms);
                self.snapshot.set_monitor_running(source_name, language, self.clocks.snapshot());
            }
            MonitorEvent::SessionStopping => {
                drop(self.recording.take());
            }
            MonitorEvent::SessionStopped => {
                if self.clocks.running() {
                    self.clocks.stop_session(at_ms);
                    self.snapshot.set_monitor_stopped(self.clocks.snapshot());
                }
            }
            _ if !self.clocks.running() => {}
            MonitorEvent::RawMatchObserved { matched, now } => {
                self.recording
                    .as_mut()
                    .expect("active recording")
                    .handle(RecordingEvent::FrameMatched { matched, now });
            }
            MonitorEvent::LanguageChanged(language) => {
                self.snapshot.set_monitor_language(language.clone());
                self.recording.as_mut().expect("active recording").handle(RecordingEvent::LanguageChanged(language));
            }
            MonitorEvent::SaveDeadlineReached(now) => {
                self.recording.as_mut().expect("active recording").handle(RecordingEvent::DeadlineReached(now));
            }
            MonitorEvent::DisplayMatchObserved(level_match) => {
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
        // The same stop path handles normal shutdown and worker unwinding.
        self.handle(MonitorEvent::SessionStopping, unix_time_ms());
        self.handle(MonitorEvent::SessionStopped, unix_time_ms());
    }
}

pub(super) fn unix_time_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "events_test.rs"]
mod tests;
