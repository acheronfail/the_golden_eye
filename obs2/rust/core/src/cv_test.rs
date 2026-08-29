use super::*;

const TEMPLATES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cv_templates");

fn solid_bgra(width: u32, height: u32, value: u8) -> Vec<u8> {
    vec![value; (width * height * 4) as usize]
}

#[test]
fn black_frame_detection_tolerates_raised_black_levels() {
    let signal =
        detect_black_frame(&solid_bgra(1920, 1080, 30), 1920, 1080, ActivePictureRegion::full(1920, 1080)).unwrap();

    assert!(signal.detected);
    assert_eq!(signal.mean_luma, 30);
    assert_eq!(signal.dark_pixel_percent, 100);
    assert_eq!(signal.sample_count, 576);
}

#[test]
fn black_frame_detection_rejects_dark_scenes_with_visible_detail() {
    let mut frame = solid_bgra(320, 180, 12);
    for pixel in frame.chunks_exact_mut(4).step_by(5) {
        pixel[..3].fill(72);
    }

    let signal = detect_black_frame(&frame, 320, 180, ActivePictureRegion::full(320, 180)).unwrap();

    assert!(!signal.detected);
    assert!(signal.dark_pixel_percent < 98);
}

#[test]
fn black_frame_detection_rejects_a_single_visible_sample() {
    let mut frame = solid_bgra(320, 180, 7);
    let sample_x = (320 / (32 * 2)) as usize;
    let sample_y = (180 / (18 * 2)) as usize;
    let offset = (sample_y * 320 + sample_x) * 4;
    frame[offset..offset + 3].fill(255);

    let signal = detect_black_frame(&frame, 320, 180, ActivePictureRegion::full(320, 180)).unwrap();

    assert_eq!(signal.dark_pixel_percent, 99);
    assert!(!signal.detected);
}

#[test]
fn black_frame_detection_rejects_invalid_buffers() {
    assert_eq!(detect_black_frame(&[], 1920, 1080, ActivePictureRegion::full(1920, 1080)), None);
    assert_eq!(detect_black_frame(&[0; 16], 0, 0, ActivePictureRegion::full(0, 0)), None);
}

#[test]
fn black_frame_detection_samples_only_the_active_picture() {
    let (width, height) = (854, 480);
    let active = ActivePictureRegion { x: 107, y: 0, width: 640, height: 480 };
    let mut frame = solid_bgra(width, height, 0);
    for y in 0..height {
        for x in active.x..active.x + active.width {
            let offset = ((y * width + x) * 4) as usize;
            frame[offset..offset + 3].fill(33);
        }
    }

    let full_signal = detect_black_frame(&frame, width, height, ActivePictureRegion::full(width, height)).unwrap();
    let active_signal = detect_black_frame(&frame, width, height, active).unwrap();

    assert!(full_signal.mean_luma < active_signal.mean_luma, "permanent bars bias the whole-frame mean");
    assert!(!full_signal.detected, "dark coverage must prevent bars alone from looking like a fade");
    assert!(!active_signal.detected, "active-picture luma should decide the fade");
    assert_eq!(active_signal.sample_region, active);
}

fn black_frame_fixture(name: &str) -> BlackFrameSignal {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../test/screenshots-rt4kce").join(name);
    let bgr = imgcodecs::imread(path.to_str().unwrap(), imgcodecs::IMREAD_COLOR).unwrap();
    let mut bgra = Mat::default();
    imgproc::cvt_color_def(&bgr, &mut bgra, imgproc::COLOR_BGR2BGRA).unwrap();
    detect_black_frame(
        bgra.data_bytes().unwrap(),
        bgra.cols() as u32,
        bgra.rows() as u32,
        ActivePictureRegion::full(bgra.cols() as u32, bgra.rows() as u32),
    )
    .unwrap()
}

#[test]
fn rt4kce_cutscene_sequence_has_exactly_three_black_frame_edges() {
    let fixtures = [
        ("jp - start - 7 - Secret Agent - fade-1-before-black.png", false),
        ("jp - unknown - fade-1-load-first-cutscene - black.png", true),
        ("jp - unknown - fade-2-first-to-second - before-black.png", false),
        ("jp - unknown - fade-2-first-to-second - black.png", true),
        ("jp - unknown - fade-3-second-to-gameplay - before-black.png", false),
        ("jp - unknown - fade-3-second-to-gameplay - black.png", true),
        ("jp - unknown - fade-3-second-to-gameplay - after-black.png", false),
    ];
    let signals = fixtures.map(|(name, expected)| {
        let signal = black_frame_fixture(name);
        assert_eq!(signal.detected, expected, "{name}");
        signal
    });
    let edges = signals
        .into_iter()
        .fold((false, 0), |(previous, edges), signal| {
            (signal.detected, edges + usize::from(signal.detected && !previous))
        })
        .1;

    assert_eq!(edges, 3);
}

fn watch_fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../test/screenshots-yt-rt4kce").join(name)
}

fn watch_fixture(name: &str) -> WatchSignal {
    let path = watch_fixture_path(name);
    let bgr = imgcodecs::imread(path.to_str().unwrap(), imgcodecs::IMREAD_COLOR).unwrap();
    let mut bgra = Mat::default();
    imgproc::cvt_color_def(&bgr, &mut bgra, imgproc::COLOR_BGR2BGRA).unwrap();
    detect_watch(
        bgra.data_bytes().unwrap(),
        bgra.cols() as u32,
        bgra.rows() as u32,
        ActivePictureRegion::full(bgra.cols() as u32, bgra.rows() as u32),
    )
    .unwrap()
}

#[test]
fn watch_diagnostics_include_developer_annotation_bounds() {
    let name = "jp - unknown - 1 - watch-menu-surface.png";
    let bytes = std::fs::read(watch_fixture_path(name)).unwrap();
    let matcher = CvMatcher::new("jp", TEMPLATES_DIR).unwrap().with_diagnostics(true);
    let (result, width, height) = matcher.match_level_from_encoded_image(&bytes).unwrap();
    let annotations = result.annotation_sets.iter().find(|set| set.id == "watch_detection").unwrap();

    assert_eq!(annotations.label, "Watch detection");
    assert_eq!(annotations.annotations.len(), 2);
    assert!(annotations.annotations[0].label.contains("MenuSurface"));
    assert!(annotations.annotations.iter().all(|annotation| {
        annotation.x >= 0
            && annotation.y >= 0
            && annotation.w > 0
            && annotation.h > 0
            && annotation.x + annotation.w <= width as i32
            && annotation.y + annotation.h <= height as i32
    }));
}

#[test]
fn watch_presentation_separates_clock_and_menu_surfaces_across_captures() {
    let fixtures = [
        ("en - unknown - 1 - watch-clock-face.png", WatchPresentation::ClockFace),
        ("en - unknown - 2 - watch-clock-face.png", WatchPresentation::ClockFace),
        ("en - unknown - 1 - watch-menu-surface.png", WatchPresentation::MenuSurface),
        ("en - unknown - 2 - watch-menu-surface.png", WatchPresentation::MenuSurface),
        ("en - unknown - 3 - watch-menu-surface.png", WatchPresentation::MenuSurface),
        ("en - unknown - 4 - watch-absent.png", WatchPresentation::Absent),
        ("en - unknown - 5 - watch-absent.png", WatchPresentation::Absent),
        ("jp - unknown - 1 - watch-clock-face.png", WatchPresentation::ClockFace),
        ("jp - unknown - 2 - watch-clock-face.png", WatchPresentation::ClockFace),
        ("jp - unknown - 3 - watch-clock-face.png", WatchPresentation::ClockFace),
        ("jp - unknown - 4 - watch-clock-face.png", WatchPresentation::ClockFace),
        ("jp - unknown - 1 - watch-menu-surface.png", WatchPresentation::MenuSurface),
        ("jp - unknown - 2 - watch-menu-surface.png", WatchPresentation::MenuSurface),
        ("jp - unknown - 3 - watch-menu-surface.png", WatchPresentation::MenuSurface),
        ("jp - unknown - 4 - watch-menu-surface.png", WatchPresentation::MenuSurface),
        ("jp - unknown - 5 - watch-menu-surface.png", WatchPresentation::MenuSurface),
        ("jp - unknown - 6 - watch-menu-surface.png", WatchPresentation::MenuSurface),
    ];

    for (name, expected) in fixtures {
        assert_eq!(watch_fixture(name).presentation, expected, "{name}");
    }
}

#[test]
fn watch_detector_latches_pause_through_menu_transitions_and_resumes_on_clock() {
    let sequences = [
        [
            "jp - unknown - 1 - watch-clock-face.png",
            "jp - unknown - 1 - watch-menu-surface.png",
            "jp - unknown - 2 - watch-menu-surface.png",
            "jp - unknown - 3 - watch-menu-surface.png",
            "jp - unknown - 2 - watch-clock-face.png",
        ],
        [
            "jp - unknown - 1 - watch-clock-face.png",
            "jp - unknown - 1 - watch-menu-surface.png",
            "jp - unknown - 2 - watch-menu-surface.png",
            "jp - unknown - 3 - watch-menu-surface.png",
            "jp - unknown - 2 - watch-clock-face.png",
        ],
    ];

    for sequence in sequences {
        let mut detector = WatchDetector::default();
        let states = sequence.map(|name| detector.observe(watch_fixture(name)));
        assert_eq!(states[0], WatchState { is_paused: false, transition: None });
        assert_eq!(states[1], WatchState { is_paused: true, transition: Some(WatchTransition::Paused) });
        assert_eq!(states[2], WatchState { is_paused: true, transition: None });
        assert_eq!(states[3], WatchState { is_paused: true, transition: None });
        assert_eq!(states[4], WatchState { is_paused: false, transition: Some(WatchTransition::Resumed) });
    }
}

#[test]
fn watch_detector_finds_both_english_pause_cycles() {
    let fixtures = [
        "en - unknown - 1 - watch-clock-face.png",
        "en - unknown - 1 - watch-menu-surface.png",
        "en - unknown - 2 - watch-menu-surface.png",
        "en - unknown - 3 - watch-menu-surface.png",
        "en - unknown - 4 - watch-absent.png",
        "en - unknown - 2 - watch-clock-face.png",
        "en - unknown - 1 - watch-absent.png",
        "en - unknown - 5 - watch-absent.png",
        "en - unknown - 6 - watch-menu-surface.png",
        "en - unknown - 2 - watch-absent.png",
        "en - unknown - 3 - watch-clock-face.png",
        "en - unknown - 7 - watch-menu-surface.png",
        "en - unknown - 8 - watch-menu-surface.png",
        "en - unknown - 9 - watch-menu-surface.png",
        "en - unknown - 4 - watch-clock-face.png",
    ];
    let mut detector = WatchDetector::default();
    let transitions: Vec<_> = fixtures
        .into_iter()
        .filter_map(|name| detector.observe(watch_fixture(name)).transition.map(|transition| (name, transition)))
        .collect();

    assert_eq!(
        transitions,
        vec![
            ("en - unknown - 1 - watch-menu-surface.png", WatchTransition::Paused),
            ("en - unknown - 2 - watch-clock-face.png", WatchTransition::Resumed),
            ("en - unknown - 7 - watch-menu-surface.png", WatchTransition::Paused),
            ("en - unknown - 4 - watch-clock-face.png", WatchTransition::Resumed),
        ]
    );
}

#[test]
fn active_picture_detection_finds_bars_on_every_edge() {
    let expected = Rect::new(107, 30, 640, 420);
    let mut gray = Mat::new_rows_cols_with_default(480, 854, core::CV_8UC1, core::Scalar::all(0.0)).unwrap();
    imgproc::rectangle(&mut gray, expected, core::Scalar::all(100.0), imgproc::FILLED, imgproc::LINE_8, 0).unwrap();

    assert_eq!(detect_active_picture(&gray).unwrap(), expected);
}

#[test]
fn pillarboxed_fixture_separates_active_picture_from_matcher_geometry() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../test/screenshots-av2hdmi/en - start - 3 - 00 Agent - blackbars.png"
    );
    let matcher = CvMatcher::new("en", TEMPLATES_DIR).unwrap();
    let bytes = std::fs::read(path).unwrap();
    let (level_match, width, height) = matcher.match_level_from_encoded_image(&bytes).unwrap();
    let active = matcher.active_picture_region(width, height);

    assert_eq!(level_match.screen, Screen::Start);
    assert!(active.x > 0);
    assert!(active.width < width);
    assert_eq!(active.height, height);
    assert!(matcher.capture_region().is_none(), "correctly shaped game pixels need no matcher correction");
}

// Decoding + matching an encoded image (the developer upload path) reads the
// same result as the file-based matcher; uses a committed flicker fixture.
#[test]
fn match_level_from_encoded_image_decodes_and_matches() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../test/screenshots-rt4kce/en - stats - 3 - Agent - 0028_0500_0028 - flicker-004.png"
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
        let mut result = level_match(screen, 1, 1, ge::Difficulty::Agent.number(), raw_times);

        reject_untrusted_screen(&mut result);

        assert_eq!(result.screen, screen, "{screen:?} should remain trusted with all markers");
    }
}

#[test]
fn overlay_screens_are_rejected_when_any_required_marker_is_missing() {
    let screens = [Screen::Start, Screen::Stats, Screen::Complete, Screen::Failed, Screen::Abort, Screen::Kia];
    let marker_cases = [(-1, 1, ge::Difficulty::Agent.number()), (1, -1, ge::Difficulty::Agent.number()), (1, 1, -1)];

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
    let mut result = level_match(Screen::Stats, 1, 1, ge::Difficulty::Agent.number(), Vec::new());

    reject_untrusted_screen(&mut result);

    assert_eq!(result.screen, Screen::Unknown);
}

#[test]
fn stats_screen_is_rejected_when_the_start_tab_is_visible() {
    let mut result = level_match(Screen::Stats, 1, 1, ge::Difficulty::Agent.number(), vec![62]);
    result.detected_lang = Some("jp".to_owned());

    reject_untrusted_screen(&mut result);

    assert_eq!(result.screen, Screen::Unknown);
    assert!(result.raw_times.is_empty());
    assert_eq!(result.times, None);
}

#[test]
fn non_overlay_screens_do_not_require_header_markers() {
    for screen in [Screen::Opts007, Screen::Select, Screen::Levels, Screen::Unknown] {
        let mut result = level_match(screen, -1, -1, -1, Vec::new());

        reject_untrusted_screen(&mut result);

        assert_eq!(result.screen, screen, "{screen:?} should not require mission/part/difficulty markers");
    }
}
