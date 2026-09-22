use super::*;

fn level_match(screen: ge_cv::Screen, mission: i32, part: i32) -> LevelMatch {
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
fn monitor_wall_clocks_follow_backend_screen_transitions() {
    let mut clocks = InGameTimer::default();

    clocks.reconcile_screen(ge_cv::Screen::Start, 1_100);
    clocks.reconcile_screen(ge_cv::Screen::Unknown, 1_250);
    assert_eq!(clocks.snapshot.level_started_at_unix_ms, None);
    assert_eq!(clocks.snapshot.level_elapsed_ms, 0);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::AwaitingInitialBlack);
    assert!(!clocks.snapshot.level_running);

    clocks.reconcile_screen(ge_cv::Screen::Stats, 3_750);
    assert_eq!(clocks.snapshot.level_elapsed_ms, 0);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::Stopped);
    assert!(!clocks.snapshot.level_running);

    clocks.reconcile_screen(ge_cv::Screen::Unknown, 4_000);
    assert_eq!(clocks.snapshot.level_elapsed_ms, 0);
    assert!(!clocks.snapshot.level_running);

    clocks.stop_level(5_000);
}

#[test]
fn monitor_wall_clock_treats_007_options_as_a_level_launch() {
    let mut clocks = InGameTimer::default();

    clocks.reconcile_match(&level_match(ge_cv::Screen::Opts007, 1, 1), 1_100);

    assert_eq!(clocks.snapshot.level_started_at_unix_ms, None);
    assert_eq!(clocks.snapshot.level_elapsed_ms, 0);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::AwaitingInitialBlack);
    assert_eq!(clocks.snapshot.intro_swirl_delay_ms, Some(ge_game::intro::swirl_delay_ms(ge_game::Level::Dam)));
    assert!(!clocks.snapshot.level_running);
}

#[test]
fn monitor_wall_clock_starts_on_a_skipped_second_cutscene_and_stops_on_the_next_fade() {
    let mut clocks = InGameTimer::default();
    let sample_region = ge_cv::ActivePictureRegion::full(640, 480);
    let black = ge_cv::BlackFrameSignal {
        detected: true,
        mean_luma: 14,
        dark_pixel_percent: 100,
        sample_count: 576,
        sample_region,
    };
    let visible = ge_cv::BlackFrameSignal {
        detected: false,
        mean_luma: 72,
        dark_pixel_percent: 11,
        sample_count: 576,
        sample_region,
    };

    clocks.reconcile_screen(ge_cv::Screen::Start, 1_100);
    clocks.reconcile_screen(ge_cv::Screen::Unknown, 1_200);
    clocks.observe_black_frame(black.detected, 1_300);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::AwaitingFirstCutscene);
    clocks.observe_black_frame(visible.detected, 1_400);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::AwaitingFirstCutsceneFade);
    clocks.observe_black_frame(black.detected, 2_000);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::AwaitingSecondFadeOrSwirl);
    clocks.observe_black_frame(visible.detected, 2_100);
    clocks.observe_black_frame(black.detected, 2_500);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::AwaitingGameplayAfterSkip);
    assert!(!clocks.snapshot.level_running);
    clocks.observe_black_frame(black.detected, 2_800);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::AwaitingGameplayAfterSkip);
    assert!(!clocks.snapshot.level_running, "a sustained skip fade must not start the timer");
    clocks.observe_black_frame(visible.detected, 3_000);
    assert_eq!(clocks.snapshot.level_started_at_unix_ms, Some(2_800));
    assert_eq!(clocks.snapshot.level_elapsed_ms, 200);
    assert_eq!(clocks.snapshot.level_start_reason, Some(LevelTimerStartReason::Fade));
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::Running);
    clocks.observe_black_frame(black.detected, 5_000);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::Running);
    assert!(clocks.snapshot.level_running);
    clocks.observe_black_frame(black.detected, 5_250);
    assert_eq!(clocks.snapshot.level_elapsed_ms, 2_200);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::Stopped);
    assert!(!clocks.snapshot.level_running);
}

#[test]
fn monitor_wall_clock_starts_when_the_level_swirl_delay_expires() {
    let sample_region = ge_cv::ActivePictureRegion::full(640, 480);
    let black = ge_cv::BlackFrameSignal {
        detected: true,
        mean_luma: 30,
        dark_pixel_percent: 100,
        sample_count: 576,
        sample_region,
    };
    let visible = ge_cv::BlackFrameSignal { detected: false, mean_luma: 80, dark_pixel_percent: 5, ..black };

    let mut clocks = InGameTimer::default();
    clocks.reconcile_match(&level_match(ge_cv::Screen::Start, 1, 2), 1_100);
    assert_eq!(clocks.snapshot.intro_swirl_delay_ms, Some(3_167));
    clocks.observe_black_frame(black.detected, 1_200);
    clocks.observe_black_frame(visible.detected, 1_300);
    clocks.observe_black_frame(black.detected, 2_000);
    clocks.observe_black_frame(visible.detected, 2_100);

    clocks.observe_black_frame(visible.detected, 5_266);
    assert!(!clocks.snapshot.level_running);
    clocks.observe_black_frame(visible.detected, 5_300);
    assert_eq!(clocks.snapshot.level_started_at_unix_ms, Some(5_267));
    assert_eq!(clocks.snapshot.level_elapsed_ms, 0);
    assert_eq!(clocks.snapshot.level_start_reason, Some(LevelTimerStartReason::Swirl));
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::Running);
}

#[test]
fn sustained_black_after_the_swirl_deadline_stops_at_the_first_black_frame() {
    let sample_region = ge_cv::ActivePictureRegion::full(640, 480);
    let black = ge_cv::BlackFrameSignal {
        detected: true,
        mean_luma: 30,
        dark_pixel_percent: 100,
        sample_count: 576,
        sample_region,
    };
    let visible = ge_cv::BlackFrameSignal { detected: false, mean_luma: 80, dark_pixel_percent: 5, ..black };

    let mut clocks = InGameTimer::default();
    clocks.reconcile_match(&level_match(ge_cv::Screen::Start, 1, 2), 1_100);
    clocks.observe_black_frame(black.detected, 1_200);
    clocks.observe_black_frame(visible.detected, 1_300);
    clocks.observe_black_frame(black.detected, 2_000);
    clocks.observe_black_frame(visible.detected, 2_100);

    clocks.observe_black_frame(black.detected, 5_300);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::Running);
    clocks.observe_black_frame(black.detected, 5_550);
    assert_eq!(clocks.snapshot.level_start_reason, Some(LevelTimerStartReason::Swirl));
    assert_eq!(clocks.snapshot.level_elapsed_ms, 33);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::Stopped);
    assert!(!clocks.snapshot.level_running);
}

#[test]
fn short_camera_black_flash_does_not_stop_the_level_timer() {
    let sample_region = ge_cv::ActivePictureRegion::full(640, 480);
    let black = ge_cv::BlackFrameSignal {
        detected: true,
        mean_luma: 7,
        dark_pixel_percent: 100,
        sample_count: 576,
        sample_region,
    };
    let visible = ge_cv::BlackFrameSignal { detected: false, mean_luma: 80, dark_pixel_percent: 5, ..black };

    let mut clocks = InGameTimer::default();
    clocks.start_level(2_000, LevelTimerStartReason::Fade);
    clocks.observe_black_frame(black.detected, 5_000);
    clocks.observe_black_frame(black.detected, 5_033);
    clocks.observe_black_frame(black.detected, 5_067);
    clocks.observe_black_frame(black.detected, 5_100);
    clocks.observe_black_frame(visible.detected, 5_133);

    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::Running);
    assert!(clocks.snapshot.level_running);
    assert_eq!(clocks.snapshot.level_started_at_unix_ms, Some(2_000));
}

#[test]
fn watch_transitions_pause_and_resume_the_running_level_clock_at_the_observed_frames() {
    let mut clocks = InGameTimer::default();
    clocks.start_level(2_000, LevelTimerStartReason::Fade);

    assert!(clocks.reconcile_watch_transition(ge_cv::WatchTransition::Paused, 3_033));
    assert_eq!(clocks.snapshot.level_elapsed_ms, 1_033);
    assert_eq!(clocks.snapshot.level_started_at_unix_ms, None);
    assert!(!clocks.snapshot.level_running);
    assert!(clocks.snapshot.level_paused);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::Running);

    assert!(clocks.reconcile_watch_transition(ge_cv::WatchTransition::Resumed, 5_033));
    assert_eq!(clocks.snapshot.level_started_at_unix_ms, Some(4_000));
    assert!(clocks.snapshot.level_running);
    assert!(!clocks.snapshot.level_paused);

    clocks.stop_level(6_000);
    assert_eq!(clocks.snapshot.level_elapsed_ms, 2_000);
}

#[test]
fn black_fade_confirmation_does_not_stop_a_watch_paused_level() {
    let sample_region = ge_cv::ActivePictureRegion::full(640, 480);
    let black = ge_cv::BlackFrameSignal {
        detected: true,
        mean_luma: 7,
        dark_pixel_percent: 100,
        sample_count: 576,
        sample_region,
    };
    let mut clocks = InGameTimer::default();
    clocks.start_level(2_000, LevelTimerStartReason::Fade);
    clocks.reconcile_watch_transition(ge_cv::WatchTransition::Paused, 3_000);

    clocks.observe_black_frame(black.detected, 3_100);
    clocks.observe_black_frame(black.detected, 3_500);

    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::Running);
    assert!(clocks.snapshot.level_paused);
    assert_eq!(clocks.snapshot.level_elapsed_ms, 1_000);
}

#[test]
fn start_match_selects_the_level_intro_swirl_delay() {
    let mut clocks = InGameTimer::default();

    clocks.reconcile_match(&level_match(ge_cv::Screen::Start, 7, 4), 1_100);

    assert_eq!(clocks.snapshot.intro_swirl_delay_ms, Some(ge_game::intro::swirl_delay_ms(ge_game::Level::Cradle)));

    clocks.reconcile_match(&level_match(ge_cv::Screen::Start, -1, -1), 1_200);
    assert_eq!(clocks.snapshot.intro_swirl_delay_ms, None);
}

#[test]
fn monitor_wall_clocks_reset_on_the_next_start_screen() {
    let mut clocks = InGameTimer::default();

    clocks.start_level(1_200, LevelTimerStartReason::Fade);
    clocks.reconcile_screen(ge_cv::Screen::Stats, 2_200);
    assert_eq!(clocks.snapshot.level_elapsed_ms, 1_000);

    clocks.reconcile_screen(ge_cv::Screen::Start, 3_000);
    assert_eq!(clocks.snapshot.level_elapsed_ms, 0);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::AwaitingInitialBlack);
    assert!(!clocks.snapshot.level_running);
}
