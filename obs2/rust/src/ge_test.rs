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
