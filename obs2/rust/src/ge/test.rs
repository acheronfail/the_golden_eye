use super::*;

#[test]
fn level_info_uses_display_names_and_one_based_numbers() {
    assert_eq!(Level::from_mission_and_part(1, 1), Some(Level::Dam));
    assert_eq!(Level::from_mission_and_part(9, 1), Some(Level::Egypt));
    assert_eq!(Level::from_mission_and_part(10, 1), None);
    assert_eq!(level_info(1, 1), Some(LevelInfo { name: "Dam", number: 1 }));
    assert_eq!(level_info(1, 2), Some(LevelInfo { name: "Facility", number: 2 }));
    assert_eq!(level_info(9, 1), Some(LevelInfo { name: "Egypt", number: 20 }));
}

#[test]
fn level_info_by_name_uses_the_canonical_level_list() {
    assert_eq!(level_info_by_name(" Facility "), Some(LevelInfo { name: "Facility", number: 2 }));
    assert_eq!(level_info_by_name("egypt"), Some(LevelInfo { name: "Egypt", number: 20 }));
    assert_eq!(level_info_by_name("Citadel"), None);
    for (index, level) in Level::ALL.into_iter().enumerate() {
        assert_eq!(level.number(), index as i32 + 1);
        assert_eq!(Level::from_name(level.name()), Some(level));
    }
}

#[test]
fn difficulty_name_uses_menu_labels() {
    assert_eq!(difficulty_name(Difficulty::Agent.number()), Some("Agent"));
    assert_eq!(difficulty_name(Difficulty::SecretAgent.number()), Some("Secret Agent"));
    assert_eq!(difficulty_name(Difficulty::Agent00.number()), Some("00 Agent"));
    assert_eq!(difficulty_name(Difficulty::Agent007.number()), Some("007"));
    assert_eq!(difficulty_name(4), None);
}

#[test]
fn difficulty_number_normalizes_metadata_labels() {
    assert_eq!(difficulty_number("Agent"), Some(Difficulty::Agent.number()));
    assert_eq!(difficulty_number(" secret AGENT "), Some(Difficulty::SecretAgent.number()));
    assert_eq!(difficulty_number("00 agent"), Some(Difficulty::Agent00.number()));
    assert_eq!(difficulty_number("007"), Some(Difficulty::Agent007.number()));
    assert_eq!(difficulty_number("unknown"), None);
}

#[test]
fn target_rows_use_typed_level_and_difficulty_rules() {
    assert_eq!(level_target(Level::Dam), (Difficulty::SecretAgent, 160));
    assert!(shows_target(Level::Dam, Difficulty::SecretAgent));
    assert!(!shows_target(Level::Dam, Difficulty::Agent));

    assert_eq!(
        Times::classify(1, 1, Difficulty::SecretAgent.number(), &[100, 160, 90]),
        Some(Times { time: 100, target_time: Some(160), best_time: Some(90) })
    );
    assert_eq!(
        Times::classify(1, 1, Difficulty::Agent.number(), &[100, 90]),
        Some(Times { time: 100, target_time: None, best_time: Some(90) })
    );
}
