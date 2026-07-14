//! GoldenEye's stats screen shows 1-3 times. The first (top to bottom, left to
//! right) is always the time, the 2nd is the target (if applicable) and the 3rd
//! the best time (if one is present).

use serde::Serialize;

/// Difficulty as the matcher reports it (`LevelMatch.difficulty`): the index of
/// the matched difficulty label, easiest first.
pub const AGENT: i32 = 0;
pub const SECRET_AGENT: i32 = 1;
pub const AGENT_00: i32 = 2;
pub const AGENT_007: i32 = 3;

const LEVELS: &[&[&str]] = &[
    &["Dam", "Facility", "Runway"],
    &["Surface 1", "Bunker 1"],
    &["Silo"],
    &["Frigate"],
    &["Surface 2", "Bunker 2"],
    &["Statue", "Archives", "Streets", "Depot", "Train"],
    &["Jungle", "Control", "Caverns", "Cradle"],
    &["Aztec"],
    &["Egypt"],
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelInfo {
    pub name: &'static str,
    pub number: i32,
}

/// Human-readable level metadata keyed by the matcher mission/part numbers.
pub fn level_info(mission: i32, part: i32) -> Option<LevelInfo> {
    let mission_idx = usize::try_from(mission.checked_sub(1)?).ok()?;
    let part_idx = usize::try_from(part.checked_sub(1)?).ok()?;
    let name = *LEVELS.get(mission_idx)?.get(part_idx)?;
    let previous_level_count = LEVELS.iter().take(mission_idx).map(|levels| levels.len()).sum::<usize>();
    Some(LevelInfo { name, number: i32::try_from(previous_level_count + part_idx + 1).ok()? })
}

/// Human-readable difficulty label keyed by the matcher difficulty index.
pub fn difficulty_name(difficulty: i32) -> Option<&'static str> {
    match difficulty {
        AGENT => Some("Agent"),
        SECRET_AGENT => Some("Secret Agent"),
        AGENT_00 => Some("00 Agent"),
        AGENT_007 => Some("007"),
        _ => None,
    }
}

/// A target time expressed as minutes:seconds, in seconds.
const fn mmss(minutes: i32, seconds: i32) -> i32 {
    minutes * 60 + seconds
}

/// The difficulty a level's target (par) time is set for, plus the target in seconds.
/// The target row prints only when completed on this exact difficulty; `None` = header
/// misread. Keyed by mission/part the matcher reads (mapping matches `test/levels.ts`).
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
    /// Classifies raw stats-screen times (top-to-bottom) into run/target/best using
    /// mission/part/difficulty to pick the row layout (see module docs). Returns
    /// `None` when no run time was read (e.g. a non-stats screen).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_info_uses_display_names_and_one_based_numbers() {
        assert_eq!(level_info(1, 1), Some(LevelInfo { name: "Dam", number: 1 }));
        assert_eq!(level_info(1, 2), Some(LevelInfo { name: "Facility", number: 2 }));
        assert_eq!(level_info(9, 1), Some(LevelInfo { name: "Egypt", number: 20 }));
        assert_eq!(level_info(10, 1), None);
    }

    #[test]
    fn difficulty_name_uses_menu_labels() {
        assert_eq!(difficulty_name(AGENT), Some("Agent"));
        assert_eq!(difficulty_name(SECRET_AGENT), Some("Secret Agent"));
        assert_eq!(difficulty_name(AGENT_00), Some("00 Agent"));
        assert_eq!(difficulty_name(AGENT_007), Some("007"));
        assert_eq!(difficulty_name(4), None);
    }
}
