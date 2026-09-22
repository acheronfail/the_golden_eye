//! The gameplay workflow: raw matches drive recording, smoothed matches drive the
//! display and timer, and shutdown flushes pending clips before freezing clocks.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use ge_cv::{BlackFrameSignal, LevelMatch, WatchTransition};

use super::RunRecorder;
use super::clocks::MonitorClocks;
use super::matcher::{DisplayTimeSmoother, log_level_match};
use super::publication::MonitorPublisher;
use crate::app::SharedStateStore;

pub(super) struct RunSession {
    recording: Option<RunRecorder>,
    clocks: MonitorClocks,
    display_smoother: DisplayTimeSmoother,
    last_display: Option<LevelMatch>,
    snapshot: MonitorPublisher,
}

impl RunSession {
    pub(super) fn new(snapshot: SharedStateStore, recording: RunRecorder) -> Self {
        Self {
            recording: Some(recording),
            clocks: MonitorClocks::default(),
            display_smoother: DisplayTimeSmoother::new(),
            last_display: None,
            snapshot: MonitorPublisher::new(snapshot),
        }
    }

    pub(super) fn start(&mut self, source_name: String, language: String) {
        self.clocks.start_session(unix_time_ms());
        self.snapshot.set_monitor_running(source_name, language, self.clocks.snapshot());
    }

    pub(super) fn process_match(&mut self, now: Instant, matched: LevelMatch) {
        if !self.clocks.running() {
            return;
        }
        // Recording votes on every raw reading, even when the displayed result is unchanged.
        self.recording.as_mut().expect("active recording").on_frame(now, &matched);

        let mut display = matched;
        display.times = self.display_smoother.smooth(&display);
        if self.last_display.as_ref().is_none_or(|previous| !previous.same_state(&display)) {
            log_level_match(&display);
            self.last_display = Some(display.clone());
            self.clocks.reconcile_match(&display, unix_time_ms());
            self.snapshot.set_match(Some(display), self.clocks.snapshot());
        }
    }

    pub(super) fn observe_black_frame(&mut self, signal: BlackFrameSignal, at_ms: u64) {
        if self.clocks.running() && self.clocks.reconcile_black_frame(signal, at_ms) {
            self.snapshot.set_monitor_wall_clocks(self.clocks.snapshot());
        }
    }

    pub(super) fn observe_watch(&mut self, transition: WatchTransition, at_ms: u64) {
        if self.clocks.running() && self.clocks.reconcile_watch_transition(transition, at_ms) {
            self.snapshot.set_monitor_wall_clocks(self.clocks.snapshot());
        }
    }

    pub(super) fn set_language(&mut self, language: String) {
        self.snapshot.set_monitor_language(language.clone());
        self.recording.as_mut().expect("active recording").set_game_language(language);
        self.refresh_display();
    }

    pub(super) fn refresh_display(&mut self) {
        self.last_display = None;
    }

    pub(super) fn pending_fire_at(&self) -> Option<Instant> {
        self.recording.as_ref().and_then(RunRecorder::pending_fire_at)
    }

    pub(super) fn poll_pending(&mut self, now: Instant) {
        if let Some(recording) = self.recording.as_mut() {
            recording.poll_pending(now);
        }
    }

    pub(super) fn stop(&mut self) {
        // Dropping the recorder waits for any pending clip's padding and saves it.
        drop(self.recording.take());
        if self.clocks.running() {
            self.clocks.stop_session(unix_time_ms());
            self.snapshot.set_monitor_stopped(self.clocks.snapshot());
        }
    }
}

impl Drop for RunSession {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(super) fn unix_time_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "session_test.rs"]
mod tests;
