use super::*;

const TEMPLATES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../cv_templates");

// Decoding + matching an encoded image (the developer upload path) reads the
// same result as the file-based matcher; uses a committed flicker fixture.
#[test]
fn match_level_from_encoded_image_decodes_and_matches() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/screenshots-rt4kce/en - stats - 3 - Agent - 0028_0500_0028 - flicker-004.png"
    );
    let bytes = std::fs::read(path).expect("read fixture");
    let matcher = CvMatcher::new("en", TEMPLATES_DIR).expect("matcher");
    let (m, w, h) = matcher.match_level_from_encoded_image(&bytes).expect("decode+match");
    assert!(w > 0 && h > 0, "decoded dimensions");
    assert_eq!(m.screen, Screen::Stats);
    assert_eq!(m.times.map(|t| t.best_time), Some(Some(28)));
}

fn level_match(screen: Screen, mission: i32, part: i32, difficulty: i32, raw_times: Vec<i32>) -> LevelMatch {
    LevelMatch {
        screen,
        mission,
        part,
        difficulty,
        detected_lang: None,
        times: ge::Times::classify(mission, part, difficulty, &raw_times),
        raw_times,
        match_regions: Vec::new(),
        annotation_sets: Vec::new(),
        runtime_ms: 0.0,
    }
}

#[test]
fn overlay_screens_with_complete_markers_remain_trusted() {
    let cases = [
        (Screen::Start, Vec::new()),
        (Screen::Stats, vec![62]),
        (Screen::Complete, Vec::new()),
        (Screen::Failed, Vec::new()),
        (Screen::Abort, Vec::new()),
        (Screen::Kia, Vec::new()),
    ];

    for (screen, raw_times) in cases {
        let mut result = level_match(screen, 1, 1, ge::AGENT, raw_times);

        reject_untrusted_screen(&mut result);

        assert_eq!(result.screen, screen, "{screen:?} should remain trusted with all markers");
    }
}

#[test]
fn overlay_screens_are_rejected_when_any_required_marker_is_missing() {
    let screens = [Screen::Start, Screen::Stats, Screen::Complete, Screen::Failed, Screen::Abort, Screen::Kia];
    let marker_cases = [(-1, 1, ge::AGENT), (1, -1, ge::AGENT), (1, 1, -1)];

    for screen in screens {
        for (mission, part, difficulty) in marker_cases {
            let raw_times = if screen == Screen::Stats { vec![62] } else { Vec::new() };
            let mut result = level_match(screen, mission, part, difficulty, raw_times);

            reject_untrusted_screen(&mut result);

            assert_eq!(result.screen, Screen::Unknown, "{screen:?} should reject incomplete markers");
            assert_eq!(result.raw_times, Vec::<i32>::new());
            assert_eq!(result.times, None);
        }
    }
}

#[test]
fn stats_screen_is_rejected_without_a_readable_run_time() {
    let mut result = level_match(Screen::Stats, 1, 1, ge::AGENT, Vec::new());

    reject_untrusted_screen(&mut result);

    assert_eq!(result.screen, Screen::Unknown);
}

#[test]
fn non_overlay_screens_do_not_require_header_markers() {
    for screen in [Screen::Opts007, Screen::Select, Screen::Levels, Screen::Unknown] {
        let mut result = level_match(screen, -1, -1, -1, Vec::new());

        reject_untrusted_screen(&mut result);

        assert_eq!(result.screen, screen, "{screen:?} should not require mission/part/difficulty markers");
    }
}
