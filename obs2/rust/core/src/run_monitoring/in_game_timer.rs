//! Estimated in-game time driven by screen, fade, and watch observations.

use serde::Serialize;

use crate::cv::{LevelMatch, WatchTransition};

const END_FADE_CONFIRMATION_MS: u64 = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum LevelTimerStartReason {
    Fade,
    Swirl,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct InGameTimerSnapshot {
    pub(crate) level_started_at_unix_ms: Option<u64>,
    pub(crate) level_elapsed_ms: u64,
    pub(crate) level_running: bool,
    pub(crate) level_paused: bool,
    pub(crate) level_start_reason: Option<LevelTimerStartReason>,
    pub(crate) level_timer_phase: LevelTimerPhase,
    pub(crate) intro_swirl_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct InGameTimer {
    snapshot: InGameTimerSnapshot,
    second_cutscene_started_at_ms: Option<u64>,
    second_cutscene_visible: bool,
    black_frame_active: bool,
    end_fade_started_at_ms: Option<u64>,
}

impl InGameTimer {
    pub(crate) fn snapshot(&self) -> &InGameTimerSnapshot {
        &self.snapshot
    }

    fn reconcile_screen(&mut self, screen: crate::cv::Screen, now_ms: u64) {
        match screen {
            screen if screen.is_level_launch() => {
                self.snapshot.level_started_at_unix_ms = None;
                self.snapshot.level_elapsed_ms = 0;
                self.snapshot.level_running = false;
                self.snapshot.level_paused = false;
                self.snapshot.level_start_reason = None;
                self.snapshot.level_timer_phase = LevelTimerPhase::AwaitingInitialBlack;
                self.snapshot.intro_swirl_delay_ms = None;
                self.second_cutscene_started_at_ms = None;
                self.second_cutscene_visible = false;
                self.black_frame_active = false;
                self.end_fade_started_at_ms = None;
            }
            crate::cv::Screen::Unknown => {}
            _ => {
                self.stop_level(now_ms);
            }
        }
    }

    pub(crate) fn reconcile_match(&mut self, level_match: &LevelMatch, now_ms: u64) {
        self.reconcile_screen(level_match.screen, now_ms);
        if level_match.screen.is_level_launch() {
            self.snapshot.intro_swirl_delay_ms =
                crate::ge::Level::from_mission_and_part(level_match.mission, level_match.part)
                    .map(crate::ge::intro::swirl_delay_ms);
        }
    }

    pub(crate) fn observe_black_frame(&mut self, black: bool, now_ms: u64) -> bool {
        let edge = black != self.black_frame_active;
        self.black_frame_active = black;
        self.reconcile_level_timer(black, edge, now_ms)
    }

    fn reconcile_level_timer(&mut self, black: bool, edge: bool, now_ms: u64) -> bool {
        let previous_phase = self.snapshot.level_timer_phase;
        if let Some(deadline) = self.pending_swirl_deadline()
            && now_ms >= deadline
        {
            self.start_level(deadline, LevelTimerStartReason::Swirl);
            if edge && black {
                self.end_fade_started_at_ms = Some(now_ms);
            }
            return self.snapshot.level_timer_phase != previous_phase;
        }

        if self.snapshot.level_timer_phase == LevelTimerPhase::Running {
            if self.snapshot.level_paused {
                self.end_fade_started_at_ms = None;
                return self.snapshot.level_timer_phase != previous_phase;
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
            return self.snapshot.level_timer_phase != previous_phase;
        }

        if edge {
            match (self.snapshot.level_timer_phase, black) {
                (LevelTimerPhase::AwaitingInitialBlack, true) => {
                    self.snapshot.level_timer_phase = LevelTimerPhase::AwaitingFirstCutscene;
                }
                (LevelTimerPhase::AwaitingFirstCutscene, false) => {
                    self.snapshot.level_timer_phase = LevelTimerPhase::AwaitingFirstCutsceneFade;
                }
                (LevelTimerPhase::AwaitingFirstCutsceneFade, true) => {
                    self.snapshot.level_timer_phase = LevelTimerPhase::AwaitingSecondFadeOrSwirl;
                    self.second_cutscene_started_at_ms = None;
                    self.second_cutscene_visible = false;
                }
                (LevelTimerPhase::AwaitingSecondFadeOrSwirl, false) => {
                    self.second_cutscene_started_at_ms = Some(now_ms);
                    self.second_cutscene_visible = true;
                }
                (LevelTimerPhase::AwaitingSecondFadeOrSwirl, true) if self.second_cutscene_visible => {
                    self.snapshot.level_timer_phase = LevelTimerPhase::AwaitingGameplayAfterSkip;
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
        self.snapshot.level_timer_phase != previous_phase
    }

    fn pending_swirl_deadline(&self) -> Option<u64> {
        if self.snapshot.level_timer_phase != LevelTimerPhase::AwaitingSecondFadeOrSwirl {
            return None;
        }
        let started_at = self.second_cutscene_started_at_ms?;
        let delay = self.snapshot.intro_swirl_delay_ms?;
        Some(started_at.saturating_add(delay))
    }

    fn start_level(&mut self, now_ms: u64, reason: LevelTimerStartReason) {
        self.start_level_with_elapsed(now_ms, 0, reason);
    }

    fn start_level_with_elapsed(&mut self, now_ms: u64, elapsed_ms: u64, reason: LevelTimerStartReason) {
        self.snapshot.level_started_at_unix_ms = Some(now_ms.saturating_sub(elapsed_ms));
        self.snapshot.level_elapsed_ms = elapsed_ms;
        self.snapshot.level_running = true;
        self.snapshot.level_paused = false;
        self.snapshot.level_start_reason = Some(reason);
        self.snapshot.level_timer_phase = LevelTimerPhase::Running;
        self.second_cutscene_started_at_ms = None;
        self.second_cutscene_visible = false;
        self.end_fade_started_at_ms = None;
    }

    pub(crate) fn stop_level(&mut self, now_ms: u64) {
        self.snapshot.level_elapsed_ms =
            elapsed_ms(self.snapshot.level_started_at_unix_ms, self.snapshot.level_elapsed_ms, now_ms);
        self.snapshot.level_started_at_unix_ms = None;
        self.snapshot.level_running = false;
        self.snapshot.level_paused = false;
        self.snapshot.level_timer_phase = LevelTimerPhase::Stopped;
        self.end_fade_started_at_ms = None;
    }

    pub(crate) fn reconcile_watch_transition(&mut self, transition: WatchTransition, now_ms: u64) -> bool {
        if self.snapshot.level_timer_phase != LevelTimerPhase::Running {
            return false;
        }
        match transition {
            WatchTransition::Paused if !self.snapshot.level_paused => {
                self.snapshot.level_elapsed_ms =
                    elapsed_ms(self.snapshot.level_started_at_unix_ms, self.snapshot.level_elapsed_ms, now_ms);
                self.snapshot.level_started_at_unix_ms = None;
                self.snapshot.level_running = false;
                self.snapshot.level_paused = true;
                self.end_fade_started_at_ms = None;
                true
            }
            WatchTransition::Resumed if self.snapshot.level_paused => {
                self.snapshot.level_started_at_unix_ms = Some(now_ms.saturating_sub(self.snapshot.level_elapsed_ms));
                self.snapshot.level_running = true;
                self.snapshot.level_paused = false;
                true
            }
            _ => false,
        }
    }
}

fn elapsed_ms(started_at_ms: Option<u64>, frozen_ms: u64, now_ms: u64) -> u64 {
    started_at_ms.map_or(frozen_ms, |started_at_ms| now_ms.saturating_sub(started_at_ms))
}

#[cfg(test)]
#[path = "in_game_timer_test.rs"]
mod tests;
