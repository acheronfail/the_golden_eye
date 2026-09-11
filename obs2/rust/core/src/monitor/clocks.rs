//! Owns the session clock and in-game timer; publishes plain HTTP snapshots.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cv::{BlackFrameSignal, LevelMatch, WatchTransition};
use crate::http::{MonitorWallClockState, SharedStateStore};
use crate::in_game_timer::InGameTimer;

const FADE_DIAGNOSTICS_INTERVAL_MS: u64 = 250;

#[derive(Default)]
struct MonitorClocks {
    display: MonitorWallClockState,
    timer: InGameTimer,
    fade_diagnostics_published_at_ms: Option<u64>,
    last_match: Option<LevelMatch>,
}

impl MonitorClocks {
    fn start_session(&mut self, now_ms: u64) {
        *self = Self {
            display: MonitorWallClockState {
                session_started_at_unix_ms: Some(now_ms),
                session_running: true,
                ..MonitorWallClockState::default()
            },
            ..Self::default()
        };
    }

    fn stop_session(&mut self, now_ms: u64) {
        self.display.session_elapsed_ms =
            elapsed_ms(self.display.session_started_at_unix_ms, self.display.session_elapsed_ms, now_ms);
        self.display.session_started_at_unix_ms = None;
        self.display.session_running = false;
        self.timer.stop_level(now_ms);
        self.sync_timer();
    }

    fn reconcile_match(&mut self, level_match: &LevelMatch, now_ms: u64) {
        self.timer.reconcile_match(level_match, now_ms);
        if level_match.screen.is_level_launch() {
            self.display.fade_detection = None;
            self.fade_diagnostics_published_at_ms = None;
        }
        self.sync_timer();
    }

    fn reconcile_black_frame(&mut self, signal: BlackFrameSignal, now_ms: u64) -> bool {
        let classification_changed =
            self.display.fade_detection.as_ref().is_some_and(|current| current.detected) != signal.detected;
        let timer_changed = self.timer.observe_black_frame(signal.detected, now_ms);
        self.sync_timer();
        let region_changed =
            self.display.fade_detection.as_ref().map(|current| current.sample_region) != Some(signal.sample_region);
        let diagnostics_changed = self.display.fade_detection != Some(signal);
        let diagnostics_due = self
            .fade_diagnostics_published_at_ms
            .is_none_or(|published_at| now_ms.saturating_sub(published_at) >= FADE_DIAGNOSTICS_INTERVAL_MS);
        if !(timer_changed || classification_changed || region_changed || diagnostics_changed && diagnostics_due) {
            return false;
        }
        self.display.fade_detection = Some(signal);
        self.fade_diagnostics_published_at_ms = Some(now_ms);
        true
    }

    fn reconcile_watch_transition(&mut self, transition: WatchTransition, now_ms: u64) -> bool {
        let changed = self.timer.reconcile_watch_transition(transition, now_ms);
        self.sync_timer();
        changed
    }

    fn sync_timer(&mut self) {
        let timer = self.timer.snapshot();
        self.display.level_started_at_unix_ms = timer.level_started_at_unix_ms;
        self.display.level_elapsed_ms = timer.level_elapsed_ms;
        self.display.level_running = timer.level_running;
        self.display.level_paused = timer.level_paused;
        self.display.level_start_reason = timer.level_start_reason;
        self.display.level_timer_phase = timer.level_timer_phase;
        self.display.intro_swirl_delay_ms = timer.intro_swirl_delay_ms;
    }
}

fn elapsed_ms(started_at_ms: Option<u64>, frozen_ms: u64, now_ms: u64) -> u64 {
    started_at_ms.map_or(frozen_ms, |started_at_ms| now_ms.saturating_sub(started_at_ms))
}

fn unix_time_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().try_into().unwrap_or(u64::MAX)
}

/// Shared by the frame worker and lifecycle owner; clock updates precede publication.
#[derive(Clone)]
pub(super) struct MonitorClockStore {
    clocks: Arc<Mutex<MonitorClocks>>,
    snapshot: SharedStateStore,
}

impl MonitorClockStore {
    pub(super) fn new(snapshot: SharedStateStore) -> Self {
        Self { clocks: Arc::new(Mutex::new(MonitorClocks::default())), snapshot }
    }

    pub(super) fn start_session(&self, source_name: String, language: String) {
        let now_ms = unix_time_ms();
        let mut clocks = self.clocks.lock().unwrap_or_else(|p| p.into_inner());
        let initial_match = clocks.last_match.take();
        clocks.start_session(now_ms);
        if let Some(level_match) = initial_match {
            clocks.reconcile_match(&level_match, now_ms);
            clocks.last_match = Some(level_match);
        }
        self.snapshot.set_monitor_running(source_name, language, clocks.display.clone());
    }

    pub(super) fn stop_session(&self) {
        let now_ms = unix_time_ms();
        let mut clocks = self.clocks.lock().unwrap_or_else(|p| p.into_inner());
        clocks.stop_session(now_ms);
        clocks.last_match = None;
        self.snapshot.set_monitor_stopped(clocks.display.clone());
    }

    pub(super) fn observe_match(&self, level_match: LevelMatch) {
        let now_ms = unix_time_ms();
        let mut clocks = self.clocks.lock().unwrap_or_else(|p| p.into_inner());
        if clocks.display.session_running {
            clocks.reconcile_match(&level_match, now_ms);
        }
        clocks.last_match = Some(level_match.clone());
        self.snapshot.set_match(Some(level_match), clocks.display.clone());
    }

    pub(super) fn observe_black_frame(&self, signal: BlackFrameSignal) {
        let now_ms = unix_time_ms();
        let mut clocks = self.clocks.lock().unwrap_or_else(|p| p.into_inner());
        if clocks.display.session_running && clocks.reconcile_black_frame(signal, now_ms) {
            self.snapshot.set_monitor_wall_clocks(clocks.display.clone());
        }
    }

    pub(super) fn observe_watch_transition(&self, transition: WatchTransition, observed_at_unix_ms: u64) {
        let mut clocks = self.clocks.lock().unwrap_or_else(|p| p.into_inner());
        if clocks.display.session_running && clocks.reconcile_watch_transition(transition, observed_at_unix_ms) {
            self.snapshot.set_monitor_wall_clocks(clocks.display.clone());
        }
    }
}

#[cfg(test)]
#[path = "clocks_test.rs"]
mod tests;
