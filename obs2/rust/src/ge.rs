//! GoldenEye level target-time definitions and stats-screen time classification.
//!
//! The stats screen prints between one and three times in a fixed top-to-bottom
//! order, and the matcher (`cv.rs`) reads them out as a flat list. This module
//! turns that flat list into the three semantic times the game actually shows --
//! the run time, the level's target (par) time, and the prior best time -- using
//! per-level knowledge of when a target time is displayed.
//!
//! The three time types are:
//!  - the time of the run (always present, always first);
//!  - the target time for the stage (only shown on the difficulty the target is
//!    set for -- see below);
//!  - the best time recorded for the stage (only shown once a prior time exists).
//!
//! What was previously believed (and is corrected here): the target time is NOT
//! a property of the stage alone. GoldenEye prints the target row on the stats
//! screen only when the level is completed on *exactly* the difficulty its
//! target is set for; on every other difficulty the target row is absent. This
//! is confirmed by the real screenshots the matcher is tested against:
//!  - Runway on Agent shows `[0:33, 5:00, 0:33]` -- 5:00 is Runway's target, and
//!    it appears because Runway's target is set for Agent, the difficulty played.
//!  - Dam on Agent shows `[1:19, 1:19]` (run + best, no target) -- Dam's target
//!    is set for Secret Agent, so no target row shows on Agent.
//!  - Dam on 00 Agent shows a single time (run only) -- again no target row,
//!    even though 00 Agent is *higher* than Dam's Secret-Agent target. So the
//!    rule is "difficulty == the level's target difficulty", not ">=".
//!
//! Once we know whether this (level, difficulty) shows a target, the layout of
//! the trailing rows is fully determined:
//!   [run]               - no target here, first completion
//!   [run, best]         - no target here, a prior best exists
//!   [run, target]       - target shown, first completion
//!   [run, target, best] - target shown, a prior best exists

use serde::Serialize;

/// Difficulty as the matcher reports it (`LevelMatch.difficulty`): the index of
/// the matched difficulty label, easiest first.
pub const AGENT: i32 = 0;
pub const SECRET_AGENT: i32 = 1;
pub const AGENT_00: i32 = 2;

/// A target time expressed as minutes:seconds, in seconds.
const fn mmss(minutes: i32, seconds: i32) -> i32 {
    minutes * 60 + seconds
}

/// The difficulty a level's target (par) time is set for, and the target itself
/// in seconds. The target row is printed on the stats screen only when the level
/// is completed on this exact difficulty. Returns `None` for an unrecognised
/// mission/part (every campaign level has a target, so a `None` here means the
/// header was misread).
///
/// Keyed by the `mission`/`part` the matcher reads off the stats-screen header;
/// the mission/part -> level mapping matches `test/levels.ts`. The level each
/// arm corresponds to is noted alongside it.
pub fn level_target(mission: i32, part: i32) -> Option<(i32, i32)> {
    let target = match (mission, part) {
        (1, 1) => (SECRET_AGENT, mmss(2, 40)), // Dam
        (1, 2) => (AGENT_00, mmss(2, 5)),      // Facility
        (1, 3) => (AGENT, mmss(5, 0)),         // Runway
        (2, 1) => (SECRET_AGENT, mmss(3, 30)), // Surface 1
        (2, 2) => (AGENT_00, mmss(4, 0)),      // Bunker 1
        (3, 1) => (AGENT, mmss(3, 0)),         // Silo
        (4, 1) => (SECRET_AGENT, mmss(4, 30)), // Frigate
        (5, 1) => (AGENT_00, mmss(4, 15)),     // Surface 2
        (5, 2) => (AGENT, mmss(1, 30)),        // Bunker 2
        (6, 1) => (SECRET_AGENT, mmss(3, 15)), // Statue
        (6, 2) => (AGENT_00, mmss(1, 20)),     // Archives
        (6, 3) => (AGENT, mmss(1, 45)),        // Streets
        (6, 4) => (SECRET_AGENT, mmss(1, 40)), // Depot
        (6, 5) => (AGENT_00, mmss(5, 25)),     // Train
        (7, 1) => (AGENT, mmss(3, 45)),        // Jungle
        (7, 2) => (SECRET_AGENT, mmss(10, 0)), // Control
        (7, 3) => (AGENT_00, mmss(9, 30)),     // Caverns
        (7, 4) => (AGENT, mmss(2, 15)),        // Cradle
        (8, 1) => (SECRET_AGENT, mmss(9, 0)),  // Aztec
        (9, 1) => (AGENT_00, mmss(6, 0)),      // Egyptian
        _ => return None,
    };
    Some(target)
}

/// Whether the stats screen for this level shows a target-time row when completed
/// on `difficulty`: only when `difficulty` matches the difficulty the level's
/// target is set for.
pub fn shows_target(mission: i32, part: i32, difficulty: i32) -> bool {
    level_target(mission, part).is_some_and(|(target_difficulty, _)| target_difficulty == difficulty)
}

/// The times shown on a completed-level stats screen, split out from the raw
/// top-to-bottom list the matcher reads off the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Times {
    /// The player's completion time for the run, in seconds. Always present.
    pub time: i32,
    /// The level's target (par) time in seconds, present only when the run was
    /// completed on the difficulty the level's target is set for.
    pub target_time: Option<i32>,
    /// The best recorded time for the level before this run, in seconds, present
    /// only once a time has been recorded on this difficulty before.
    pub best_time: Option<i32>,
}

impl Times {
    /// Classifies the raw list of times read off a stats screen (in the overlay's
    /// top-to-bottom order) into run / target / best, using the level's
    /// mission/part/difficulty to decide the row layout (see the module docs).
    ///
    /// Returns `None` when no run time was read -- e.g. a non-stats screen, which
    /// carries no timed rows.
    pub fn classify(mission: i32, part: i32, difficulty: i32, times: &[i32]) -> Option<Times> {
        let &time = times.first()?;
        let (target_time, best_time) = if shows_target(mission, part, difficulty) {
            // [run, target, best?]
            (times.get(1).copied(), times.get(2).copied())
        } else {
            // [run, best?]
            (None, times.get(1).copied())
        };
        Some(Times { time, target_time, best_time })
    }
}
