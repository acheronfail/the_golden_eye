//! Session time and fade diagnostics alongside the independently owned in-game timer.

use crate::cv::{BlackFrameSignal, LevelMatch, WatchTransition};
use crate::run_monitoring::in_game_timer::InGameTimer;
use crate::run_monitoring::publication::MonitorWallClockState;

const FADE_DIAGNOSTICS_INTERVAL_MS: u64 = 250;

#[derive(Default)]
pub(super) struct MonitorClocks {
    session_started_at_ms: Option<u64>,
    session_elapsed_ms: u64,
    timer: InGameTimer,
    fade_detection: Option<BlackFrameSignal>,
    fade_diagnostics_published_at_ms: Option<u64>,
}

impl MonitorClocks {
    pub(super) fn start_session(&mut self, now_ms: u64) {
        *self = Self { session_started_at_ms: Some(now_ms), ..Self::default() };
    }

    pub(super) fn stop_session(&mut self, now_ms: u64) {
        if let Some(started_at) = self.session_started_at_ms.take() {
            self.session_elapsed_ms = now_ms.saturating_sub(started_at);
        }
        self.timer.stop_level(now_ms);
    }

    pub(super) fn reconcile_match(&mut self, level_match: &LevelMatch, now_ms: u64) {
        self.timer.reconcile_match(level_match, now_ms);
        if level_match.screen.is_level_launch() {
            self.fade_detection = None;
            self.fade_diagnostics_published_at_ms = None;
        }
    }

    pub(super) fn reconcile_black_frame(&mut self, signal: BlackFrameSignal, now_ms: u64) -> bool {
        let classification_changed =
            self.fade_detection.as_ref().is_some_and(|current| current.detected) != signal.detected;
        let timer_changed = self.timer.observe_black_frame(signal.detected, now_ms);
        let region_changed =
            self.fade_detection.as_ref().map(|current| current.sample_region) != Some(signal.sample_region);
        let diagnostics_changed = self.fade_detection != Some(signal);
        let diagnostics_due = self
            .fade_diagnostics_published_at_ms
            .is_none_or(|published_at| now_ms.saturating_sub(published_at) >= FADE_DIAGNOSTICS_INTERVAL_MS);
        if !(timer_changed || classification_changed || region_changed || diagnostics_changed && diagnostics_due) {
            return false;
        }
        self.fade_detection = Some(signal);
        self.fade_diagnostics_published_at_ms = Some(now_ms);
        true
    }

    pub(super) fn reconcile_watch_transition(&mut self, transition: WatchTransition, now_ms: u64) -> bool {
        self.timer.reconcile_watch_transition(transition, now_ms)
    }

    pub(super) fn running(&self) -> bool {
        self.session_started_at_ms.is_some()
    }

    pub(super) fn snapshot(&self) -> MonitorWallClockState {
        let timer = self.timer.snapshot();
        MonitorWallClockState {
            session_started_at_unix_ms: self.session_started_at_ms,
            session_elapsed_ms: self.session_elapsed_ms,
            session_running: self.running(),
            level_started_at_unix_ms: timer.level_started_at_unix_ms,
            level_elapsed_ms: timer.level_elapsed_ms,
            level_running: timer.level_running,
            level_paused: timer.level_paused,
            level_start_reason: timer.level_start_reason,
            level_timer_phase: timer.level_timer_phase,
            intro_swirl_delay_ms: timer.intro_swirl_delay_ms,
            fade_detection: self.fade_detection,
        }
    }
}

#[cfg(test)]
#[path = "clocks_test.rs"]
mod tests;
