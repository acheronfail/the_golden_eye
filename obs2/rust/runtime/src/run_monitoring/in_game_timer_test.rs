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

fn observe_until(clocks: &mut InGameTimer, black: bool, from_ms: u64, until_ms: u64) {
    for at in (from_ms..until_ms).step_by(16) {
        clocks.observe_black_frame(black, at);
    }
    clocks.observe_black_frame(black, until_ms);
}

fn replay_intro_edges(clocks: &mut InGameTimer, edges: &[(u64, bool)], until_ms: u64) {
    replay_intro_edges_at_interval(clocks, edges, until_ms, 16);
}

fn replay_intro_edges_at_interval(clocks: &mut InGameTimer, edges: &[(u64, bool)], until_ms: u64, interval_ms: usize) {
    for (index, &(at, black)) in edges.iter().enumerate() {
        let next_at = edges.get(index + 1).map_or(until_ms + 1, |edge| edge.0);
        for at in (at..next_at).step_by(interval_ms) {
            clocks.observe_black_frame(black, at);
        }
        clocks.observe_black_frame(black, next_at - 1);
    }
}

fn skipped_intro(clocks: &mut InGameTimer) {
    clocks.reconcile_screen(ge_cv::Screen::Start, 1_100);
    replay_intro_edges(
        clocks,
        &[(1_200, true), (1_500, false), (2_200, true), (2_600, false), (3_100, true), (3_400, false)],
        4_400,
    );
}

#[test]
fn watch_pause_during_deferred_gameplay_fade_is_preserved() {
    for resume_at_ms in [None, Some(4_100)] {
        let mut clocks = InGameTimer::default();
        clocks.reconcile_screen(ge_cv::Screen::Start, 1_100);
        replay_intro_edges(
            &mut clocks,
            &[(1_200, true), (1_500, false), (2_200, true), (2_600, false), (3_100, true), (3_400, false)],
            3_700,
        );
        clocks.reconcile_watch_transition(WatchTransition::Paused, 3_700);
        if let Some(at_ms) = resume_at_ms {
            observe_until(&mut clocks, false, 3_701, at_ms);
            clocks.reconcile_watch_transition(WatchTransition::Resumed, at_ms);
        }
        observe_until(&mut clocks, false, resume_at_ms.unwrap_or(3_700) + 1, 4_400);
        assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::Running);
        if resume_at_ms.is_some() {
            assert!(clocks.snapshot.level_running);
            assert_eq!(clocks.snapshot.level_started_at_unix_ms, Some(3_600));
            assert_eq!(clocks.snapshot.level_elapsed_ms, 800);
        } else {
            assert!(clocks.snapshot.level_paused);
            assert!(!clocks.snapshot.level_running);
            assert_eq!(clocks.snapshot.level_elapsed_ms, 500);
            clocks.reconcile_watch_transition(WatchTransition::Resumed, 5_000);
            assert_eq!(clocks.snapshot.level_started_at_unix_ms, Some(4_500));
        }
    }
}

#[test]
fn skipped_intro_waits_for_fades_but_preserves_gameplay_start_time() {
    let mut clocks = InGameTimer::default();
    skipped_intro(&mut clocks);
    assert_eq!(clocks.snapshot.level_started_at_unix_ms, Some(3_200));
    assert_eq!(clocks.snapshot.level_elapsed_ms, 1_200);
    assert_eq!(clocks.snapshot.level_start_reason, Some(LevelTimerStartReason::Fade));
    assert!(clocks.snapshot.level_running);
    clocks.observe_black_frame(true, 5_000);
    assert!(clocks.snapshot.level_running);
    clocks.observe_black_frame(true, 5_250);
    assert_eq!(clocks.snapshot.level_elapsed_ms, 1_800);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::Stopped);
}

#[test]
fn archives_intro_flicker_does_not_start_or_stop_the_timer() {
    let mut clocks = InGameTimer::default();
    clocks.reconcile_match(&level_match(ge_cv::Screen::Start, 6, 2), 2_296);
    replay_intro_edges(
        &mut clocks,
        &[
            (2_626, true),
            (6_592, false),
            (8_243, true),
            (8_643, false),
            (10_210, true),
            (10_226, false),
            (10_260, true),
            (10_725, false),
        ],
        11_724,
    );
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::AwaitingGameplayAfterSkip);
    assert!(!clocks.snapshot.level_running);
    clocks.observe_black_frame(false, 11_725);
    assert_eq!(clocks.snapshot.level_started_at_unix_ms, Some(10_525));
    assert_eq!(clocks.snapshot.level_elapsed_ms, 1_200);
    clocks.observe_black_frame(true, 38_500);
    clocks.observe_black_frame(true, 38_750);
    assert_eq!(clocks.snapshot.level_elapsed_ms, 27_975);
}

#[test]
fn silo_fade_flicker_and_early_swirl_skip_preserve_the_intro_sequence() {
    let mut clocks = InGameTimer::default();
    clocks.reconcile_match(&level_match(ge_cv::Screen::Start, 3, 1), 2_283);
    replay_intro_edges(
        &mut clocks,
        &[
            (2_695, true),
            (7_746, false),
            (8_362, true),
            (8_379, false),
            (8_396, true),
            (9_179, false),
            (9_379, true),
            (9_829, false),
        ],
        10_178,
    );
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::AwaitingSecondFadeOrSwirl);
    assert_eq!(clocks.second_cutscene_started_at_ms, Some(9_179));
    assert!(!clocks.snapshot.level_running);
    clocks.observe_black_frame(false, 10_179);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::AwaitingGameplayAfterSkip);
    assert!(!clocks.snapshot.level_running);
    observe_until(&mut clocks, false, 10_180, 10_829);
    assert!(clocks.snapshot.level_running);
    assert_eq!(clocks.snapshot.level_started_at_unix_ms, Some(9_629));
    clocks.observe_black_frame(true, 94_062);
    clocks.observe_black_frame(true, 94_312);
    assert_eq!(clocks.snapshot.level_elapsed_ms, 84_433);
}

#[test]
fn first_cutscene_black_flicker_during_fade_in_is_not_another_scene() {
    let mut clocks = InGameTimer::default();
    clocks.reconcile_screen(ge_cv::Screen::Start, 0);
    replay_intro_edges(&mut clocks, &[(100, true), (500, false), (700, true), (717, false)], 1_600);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::AwaitingFirstCutsceneFade);
    assert_eq!(clocks.second_cutscene_started_at_ms, None);
    assert!(!clocks.snapshot.level_running);
}

#[test]
fn silo_intro_keeps_the_same_start_time_at_different_capture_rates() {
    for interval_ms in [16, 33, 50] {
        let mut clocks = InGameTimer::default();
        clocks.reconcile_match(&level_match(ge_cv::Screen::Start, 3, 1), 2_283);
        replay_intro_edges_at_interval(
            &mut clocks,
            &[
                (2_695, true),
                (7_746, false),
                (8_362, true),
                (8_379, false),
                (8_396, true),
                (9_179, false),
                (9_379, true),
                (9_829, false),
            ],
            11_000,
            interval_ms,
        );
        assert!(clocks.snapshot.level_running, "capture interval {interval_ms}");
        assert_eq!(clocks.snapshot.level_started_at_unix_ms, Some(9_629));
    }
}

#[test]
fn gameplay_fade_flicker_longer_than_the_old_confirmation_is_not_a_start() {
    let mut clocks = InGameTimer::default();
    clocks.reconcile_screen(ge_cv::Screen::Start, 0);
    replay_intro_edges(
        &mut clocks,
        &[
            (100, true),
            (500, false),
            (1_600, true),
            (1_900, false),
            (3_000, true),
            (3_300, false),
            (3_500, true),
            (3_900, false),
        ],
        4_899,
    );
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::AwaitingGameplayAfterSkip);
    assert!(!clocks.snapshot.level_running);
    clocks.observe_black_frame(false, 4_900);
    assert_eq!(clocks.snapshot.level_started_at_unix_ms, Some(3_700));
}

#[test]
fn a_new_launch_discards_pending_intro_observations() {
    let mut clocks = InGameTimer::default();
    clocks.reconcile_screen(ge_cv::Screen::Start, 0);
    replay_intro_edges(&mut clocks, &[(100, true), (500, false), (700, true), (1_000, false)], 1_100);
    assert!(!clocks.intro_observations.is_empty());
    skipped_intro(&mut clocks);
    assert_eq!(clocks.snapshot.level_started_at_unix_ms, Some(3_200));
}

fn unskipped_intro(clocks: &mut InGameTimer) {
    clocks.reconcile_match(&level_match(ge_cv::Screen::Start, 1, 2), 1_100);
    replay_intro_edges(clocks, &[(1_200, true), (1_500, false), (2_600, true), (2_900, false)], 4_000);
}

#[test]
fn monitor_wall_clock_starts_when_the_level_swirl_delay_expires() {
    let mut clocks = InGameTimer::default();
    unskipped_intro(&mut clocks);
    assert_eq!(clocks.snapshot.intro_swirl_delay_ms, Some(3_167));
    clocks.observe_black_frame(false, 6_066);
    assert!(!clocks.snapshot.level_running);
    clocks.observe_black_frame(false, 6_100);
    assert_eq!(clocks.snapshot.level_started_at_unix_ms, Some(6_067));
    assert_eq!(clocks.snapshot.level_start_reason, Some(LevelTimerStartReason::Swirl));
    assert!(clocks.snapshot.level_running);
}

#[test]
fn sustained_black_after_the_swirl_deadline_stops_at_the_first_observed_black_frame() {
    let mut clocks = InGameTimer::default();
    unskipped_intro(&mut clocks);
    clocks.observe_black_frame(true, 6_100);
    assert!(clocks.snapshot.level_running);
    clocks.observe_black_frame(true, 6_350);
    assert_eq!(clocks.snapshot.level_start_reason, Some(LevelTimerStartReason::Swirl));
    assert_eq!(clocks.snapshot.level_elapsed_ms, 33);
    assert_eq!(clocks.snapshot.level_timer_phase, LevelTimerPhase::Stopped);
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
