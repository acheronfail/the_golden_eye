use super::*;
use crate::run_monitoring::in_game_timer::LevelTimerPhase;

fn level_match(screen: crate::cv::Screen, mission: i32, part: i32) -> LevelMatch {
    LevelMatch {
        screen,
        mission,
        part,
        difficulty: 0,
        detected_lang: None,
        times: None,
        raw_times: Vec::new(),
        match_regions: Vec::new(),
        annotation_sets: Vec::new(),
        runtime_ms: 0.0,
    }
}

#[test]
fn black_frame_diagnostics_update_immediately_for_edges_and_periodically_for_evidence() {
    let mut clocks = MonitorClocks::default();
    let mut signal = crate::cv::BlackFrameSignal {
        detected: false,
        mean_luma: 80,
        dark_pixel_percent: 4,
        sample_count: 576,
        sample_region: crate::cv::ActivePictureRegion::full(854, 480),
    };

    assert!(clocks.reconcile_black_frame(signal, 1_000));
    signal.mean_luma = 70;
    assert!(!clocks.reconcile_black_frame(signal, 1_100));
    assert!(clocks.reconcile_black_frame(signal, 1_250));
    assert_eq!(clocks.snapshot().fade_detection, Some(signal));

    signal.sample_region = crate::cv::ActivePictureRegion { x: 107, y: 0, width: 640, height: 480 };
    assert!(clocks.reconcile_black_frame(signal, 1_300));
    assert_eq!(clocks.snapshot().fade_detection, Some(signal));
}

#[test]
fn wall_clock_snapshot_projects_timer_and_resets_with_session() {
    let mut clocks = MonitorClocks::default();
    clocks.start_session(1_000);
    clocks.reconcile_match(&level_match(crate::cv::Screen::Start, 1, 2), 1_100);
    let mut signal = BlackFrameSignal {
        detected: true,
        mean_luma: 0,
        dark_pixel_percent: 100,
        sample_count: 576,
        sample_region: crate::cv::ActivePictureRegion::full(640, 480),
    };
    for (time, black) in [(1_200, true), (1_300, false), (2_000, true), (2_100, false), (5_300, false)] {
        signal.detected = black;
        clocks.reconcile_black_frame(signal, time);
    }
    assert!(clocks.snapshot().level_running);
    assert_eq!(clocks.snapshot().level_started_at_unix_ms, Some(5_267));
    assert!(clocks.reconcile_watch_transition(WatchTransition::Paused, 6_000));
    let json = serde_json::to_value(&clocks.snapshot()).unwrap();
    assert_eq!(json["levelElapsedMs"], 733);
    assert_eq!(json["levelPaused"], true);
    assert_eq!(json["levelTimerPhase"], "running");
    assert_eq!(json["levelStartReason"], "swirl");
    assert_eq!(json["fadeDetection"]["sampleRegion"]["width"], 640);
    assert_eq!(json["introSwirlDelayMs"], 3_167);
    assert!(json.get("timer").is_none());
    assert!(clocks.reconcile_watch_transition(WatchTransition::Resumed, 7_000));
    clocks.stop_session(8_000);
    assert_eq!(clocks.snapshot().level_elapsed_ms, 1_733);
    assert_eq!(clocks.snapshot().session_elapsed_ms, 7_000);
    assert!(!clocks.snapshot().level_running);
    clocks.start_session(9_000);
    assert_eq!(clocks.snapshot().level_timer_phase, LevelTimerPhase::Idle);
    assert_eq!(clocks.snapshot().level_elapsed_ms, 0);
    assert_eq!(clocks.snapshot().fade_detection, None);
}
