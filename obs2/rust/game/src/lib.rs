//! GoldenEye game metadata and timing rules shared across the matcher,
//! recording pipeline, and monitor.

use serde::Serialize;

pub mod intro;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Level {
    Dam = 1,
    Facility = 2,
    Runway = 3,
    Surface1 = 4,
    Bunker1 = 5,
    Silo = 6,
    Frigate = 7,
    Surface2 = 8,
    Bunker2 = 9,
    Statue = 10,
    Archives = 11,
    Streets = 12,
    Depot = 13,
    Train = 14,
    Jungle = 15,
    Control = 16,
    Caverns = 17,
    Cradle = 18,
    Aztec = 19,
    Egypt = 20,
}

impl Level {
    pub const ALL: [Self; 20] = [
        Self::Dam,
        Self::Facility,
        Self::Runway,
        Self::Surface1,
        Self::Bunker1,
        Self::Silo,
        Self::Frigate,
        Self::Surface2,
        Self::Bunker2,
        Self::Statue,
        Self::Archives,
        Self::Streets,
        Self::Depot,
        Self::Train,
        Self::Jungle,
        Self::Control,
        Self::Caverns,
        Self::Cradle,
        Self::Aztec,
        Self::Egypt,
    ];

    pub const fn number(self) -> i32 {
        self as i32
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Dam => "Dam",
            Self::Facility => "Facility",
            Self::Runway => "Runway",
            Self::Surface1 => "Surface 1",
            Self::Bunker1 => "Bunker 1",
            Self::Silo => "Silo",
            Self::Frigate => "Frigate",
            Self::Surface2 => "Surface 2",
            Self::Bunker2 => "Bunker 2",
            Self::Statue => "Statue",
            Self::Archives => "Archives",
            Self::Streets => "Streets",
            Self::Depot => "Depot",
            Self::Train => "Train",
            Self::Jungle => "Jungle",
            Self::Control => "Control",
            Self::Caverns => "Caverns",
            Self::Cradle => "Cradle",
            Self::Aztec => "Aztec",
            Self::Egypt => "Egypt",
        }
    }

    pub fn from_number(n: i32) -> Option<Self> {
        match n {
            1 => Some(Self::Dam),
            2 => Some(Self::Facility),
            3 => Some(Self::Runway),
            4 => Some(Self::Surface1),
            5 => Some(Self::Bunker1),
            6 => Some(Self::Silo),
            7 => Some(Self::Frigate),
            8 => Some(Self::Surface2),
            9 => Some(Self::Bunker2),
            10 => Some(Self::Statue),
            11 => Some(Self::Archives),
            12 => Some(Self::Streets),
            13 => Some(Self::Depot),
            14 => Some(Self::Train),
            15 => Some(Self::Jungle),
            16 => Some(Self::Control),
            17 => Some(Self::Caverns),
            18 => Some(Self::Cradle),
            19 => Some(Self::Aztec),
            20 => Some(Self::Egypt),
            _ => None,
        }
    }

    pub fn from_mission_and_part(mission: i32, part: i32) -> Option<Self> {
        match (mission, part) {
            (1, 1) => Some(Self::Dam),
            (1, 2) => Some(Self::Facility),
            (1, 3) => Some(Self::Runway),
            (2, 1) => Some(Self::Surface1),
            (2, 2) => Some(Self::Bunker1),
            (3, 1) => Some(Self::Silo),
            (4, 1) => Some(Self::Frigate),
            (5, 1) => Some(Self::Surface2),
            (5, 2) => Some(Self::Bunker2),
            (6, 1) => Some(Self::Statue),
            (6, 2) => Some(Self::Archives),
            (6, 3) => Some(Self::Streets),
            (6, 4) => Some(Self::Depot),
            (6, 5) => Some(Self::Train),
            (7, 1) => Some(Self::Jungle),
            (7, 2) => Some(Self::Control),
            (7, 3) => Some(Self::Caverns),
            (7, 4) => Some(Self::Cradle),
            (8, 1) => Some(Self::Aztec),
            (9, 1) => Some(Self::Egypt),
            _ => None,
        }
    }

    pub fn to_mission_and_part(self) -> (i32, i32) {
        match self {
            Self::Dam => (1, 1),
            Self::Facility => (1, 2),
            Self::Runway => (1, 3),
            Self::Surface1 => (2, 1),
            Self::Bunker1 => (2, 2),
            Self::Silo => (3, 1),
            Self::Frigate => (4, 1),
            Self::Surface2 => (5, 1),
            Self::Bunker2 => (5, 2),
            Self::Statue => (6, 1),
            Self::Archives => (6, 2),
            Self::Streets => (6, 3),
            Self::Depot => (6, 4),
            Self::Train => (6, 5),
            Self::Jungle => (7, 1),
            Self::Control => (7, 2),
            Self::Caverns => (7, 3),
            Self::Cradle => (7, 4),
            Self::Aztec => (8, 1),
            Self::Egypt => (9, 1),
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|level| level.name().eq_ignore_ascii_case(name.trim()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Difficulty {
    Agent = 0,
    SecretAgent = 1,
    Agent00 = 2,
    Agent007 = 3,
}

impl Difficulty {
    pub const fn number(self) -> i32 {
        self as i32
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Agent => "Agent",
            Self::SecretAgent => "Secret Agent",
            Self::Agent00 => "00 Agent",
            Self::Agent007 => "007",
        }
    }

    pub fn from_number(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Agent),
            1 => Some(Self::SecretAgent),
            2 => Some(Self::Agent00),
            3 => Some(Self::Agent007),
            _ => None,
        }
    }

    pub fn from_name(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "agent" => Some(Self::Agent),
            "secret agent" => Some(Self::SecretAgent),
            "00 agent" => Some(Self::Agent00),
            "007" => Some(Self::Agent007),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelInfo {
    pub name: &'static str,
    pub number: i32,
}

/// Human-readable level metadata keyed by the matcher mission/part numbers.
pub fn level_info(mission: i32, part: i32) -> Option<LevelInfo> {
    Level::from_mission_and_part(mission, part).map(LevelInfo::from)
}

/// Canonical level metadata keyed by a human-readable name.
pub fn level_info_by_name(name: &str) -> Option<LevelInfo> {
    Level::from_name(name).map(LevelInfo::from)
}

impl From<Level> for LevelInfo {
    fn from(level: Level) -> Self {
        Self { name: level.name(), number: level.number() }
    }
}

/// Human-readable difficulty label keyed by the matcher difficulty index.
pub fn difficulty_name(difficulty: i32) -> Option<&'static str> {
    Difficulty::from_number(difficulty).map(Difficulty::name)
}

/// Canonical matcher difficulty keyed by a human-readable metadata label.
pub fn difficulty_number(value: &str) -> Option<i32> {
    Difficulty::from_name(value).map(Difficulty::number)
}

/// A target time expressed as minutes:seconds, in seconds.
const fn mmss(minutes: i32, seconds: i32) -> i32 {
    minutes * 60 + seconds
}

/// The difficulty a level's target time is set for, plus the target in seconds.
pub const fn level_target(level: Level) -> (Difficulty, i32) {
    match level {
        Level::Dam => (Difficulty::SecretAgent, mmss(2, 40)),
        Level::Facility => (Difficulty::Agent00, mmss(2, 5)),
        Level::Runway => (Difficulty::Agent, mmss(5, 0)),
        Level::Surface1 => (Difficulty::SecretAgent, mmss(3, 30)),
        Level::Bunker1 => (Difficulty::Agent00, mmss(4, 0)),
        Level::Silo => (Difficulty::Agent, mmss(3, 0)),
        Level::Frigate => (Difficulty::SecretAgent, mmss(4, 30)),
        Level::Surface2 => (Difficulty::Agent00, mmss(4, 15)),
        Level::Bunker2 => (Difficulty::Agent, mmss(1, 30)),
        Level::Statue => (Difficulty::SecretAgent, mmss(3, 15)),
        Level::Archives => (Difficulty::Agent00, mmss(1, 20)),
        Level::Streets => (Difficulty::Agent, mmss(1, 45)),
        Level::Depot => (Difficulty::SecretAgent, mmss(1, 40)),
        Level::Train => (Difficulty::Agent00, mmss(5, 25)),
        Level::Jungle => (Difficulty::Agent, mmss(3, 45)),
        Level::Control => (Difficulty::SecretAgent, mmss(10, 0)),
        Level::Caverns => (Difficulty::Agent00, mmss(9, 30)),
        Level::Cradle => (Difficulty::Agent, mmss(2, 15)),
        Level::Aztec => (Difficulty::SecretAgent, mmss(9, 0)),
        Level::Egypt => (Difficulty::Agent00, mmss(6, 0)),
    }
}

/// Whether this level shows a target-time row on the given difficulty.
pub fn shows_target(level: Level, difficulty: Difficulty) -> bool {
    level_target(level).0 == difficulty
}

/// The times shown on a completed-level stats screen, split out from the raw
/// top-to-bottom list the matcher reads off the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "export", derive(ts_rs::TS))]
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
        let shows_target = Level::from_mission_and_part(mission, part)
            .zip(Difficulty::from_number(difficulty))
            .is_some_and(|(level, difficulty)| shows_target(level, difficulty));
        let (target_time, best_time) = if shows_target {
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
mod test;
