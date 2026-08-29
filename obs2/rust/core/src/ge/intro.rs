//! GoldenEye's level-intro timing behavior.

use super::Level;

pub const SKIPPED_SWIRL_INITIAL_ELAPSED_MS: u64 = 200;

/// Delay from the second cutscene's first visible frame to player control.
pub const fn swirl_delay_ms(level: Level) -> u64 {
    match level {
        Level::Dam => 9_700,
        Level::Facility => 3_167,
        Level::Runway => 4_383,
        Level::Surface1 => 3_650,
        Level::Bunker1 => 2_833,
        Level::Silo => 5_350,
        Level::Frigate => 3_433,
        Level::Surface2 => 5_120,
        Level::Bunker2 => 2_850,
        Level::Statue => 5_333,
        Level::Archives => 3_800,
        Level::Streets => 2_840,
        Level::Depot => 5_350,
        Level::Train => 2_900,
        Level::Jungle => 3_783,
        Level::Control => 2_820,
        Level::Caverns => 4_483,
        Level::Cradle => 4_567,
        Level::Aztec => 2_840,
        Level::Egypt => 2_167,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_level_uses_its_measured_swirl_delay() {
        let levels = [
            (Level::Dam, 9_700),
            (Level::Facility, 3_167),
            (Level::Runway, 4_383),
            (Level::Surface1, 3_650),
            (Level::Bunker1, 2_833),
            (Level::Silo, 5_350),
            (Level::Frigate, 3_433),
            (Level::Surface2, 5_120),
            (Level::Bunker2, 2_850),
            (Level::Statue, 5_333),
            (Level::Archives, 3_800),
            (Level::Streets, 2_840),
            (Level::Depot, 5_350),
            (Level::Train, 2_900),
            (Level::Jungle, 3_783),
            (Level::Control, 2_820),
            (Level::Caverns, 4_483),
            (Level::Cradle, 4_567),
            (Level::Aztec, 2_840),
            (Level::Egypt, 2_167),
        ];

        assert!(levels.into_iter().all(|(level, delay)| swirl_delay_ms(level) == delay));
    }
}
