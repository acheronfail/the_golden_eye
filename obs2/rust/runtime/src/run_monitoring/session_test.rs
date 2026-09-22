use super::*;
use crate::run_monitoring::in_game_timer::LevelTimerPhase;
use crate::run_monitoring::publication::MonitorWallClockState;

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

fn snapshot_store() -> SharedStateStore {
    SharedStateStore::new(crate::app::AppSnapshot {
        monitor: crate::run_monitoring::publication::MonitorSnapshot {
            enabled: false,
            source_name: None,
            cv_language: None,
            wall_clocks: MonitorWallClockState::default(),
        },
        level_match: None,
        run_catalog_sync: None,
        recording_state: None,
        replay_saves: vec![],
        sources: vec![],
        replay_buffer: crate::obs::ReplayBufferStatus::unknown(),
        settings_status: crate::settings::SettingsStatus {
            settings: crate::settings::AppSettings::default(),
            defaults: crate::settings::AppSettings::default(),
            config_path: "/tmp/settings.json".to_owned(),
            plugin_version: "test".to_owned(),
            file_error: None,
        },
        update: crate::plugin_updates::UpdateStatus::default(),
    })
}

fn run_session(snapshot: SharedStateStore) -> RunSession {
    let (tx, _) = tokio::sync::broadcast::channel(8);
    let recording = crate::run_monitoring::RunRecorder::new(
        tx,
        crate::run_monitoring::RecordingStateStore::new(snapshot.clone()),
        crate::run_monitoring::publication::ReplaySaveStateStore::new(snapshot.clone()),
        crate::run_monitoring::RecordingOptions::default(),
        crate::run_monitoring::RecordingSessionContext::new("N64 Capture".to_owned(), "en".to_owned(), None),
        crate::run_monitoring::test_support::test_run_catalog("monitor-events"),
    );
    RunSession::new(snapshot, recording)
}

fn black(detected: bool) -> BlackFrameSignal {
    BlackFrameSignal {
        detected,
        mean_luma: 0,
        dark_pixel_percent: 100,
        sample_count: 576,
        sample_region: ge_cv::ActivePictureRegion::full(640, 480),
    }
}

#[test]
fn session_publishes_recording_and_timer_from_the_same_match() {
    let snapshot = snapshot_store();
    let mut session = run_session(snapshot.clone());
    session.start("N64 Capture".to_owned(), "en".to_owned());
    let now = Instant::now();
    session.process_match(now, level_match(ge_cv::Screen::Start, 1, 2));
    let matched = snapshot.current();
    assert_eq!(matched.recording_state, Some(crate::run_monitoring::RecordingStatus::Started));
    assert_eq!(matched.level_match.unwrap().screen, ge_cv::Screen::Start);
    assert_eq!(matched.monitor.wall_clocks.level_timer_phase, LevelTimerPhase::AwaitingInitialBlack);
    for (at, detected) in [(1_200, true), (1_300, false), (2_000, true), (2_100, false), (5_300, false)] {
        session.observe_black_frame(black(detected), at);
    }
    assert_eq!(snapshot.current().monitor.wall_clocks.level_started_at_unix_ms, Some(5_267));
    session.observe_watch(WatchTransition::Paused, 6_000);
    assert_eq!(snapshot.current().monitor.wall_clocks.level_elapsed_ms, 733);
    session.observe_watch(WatchTransition::Resumed, 7_000);
    assert!(snapshot.current().monitor.wall_clocks.level_running);
    session.set_language("jp".to_owned());
    assert_eq!(snapshot.current().monitor.cv_language.as_deref(), Some("jp"));
    session.stop();
    let stopped = snapshot.current();
    assert!(!stopped.monitor.enabled);
    assert_eq!(stopped.monitor.wall_clocks.level_timer_phase, LevelTimerPhase::Stopped);
    assert!(stopped.level_match.is_none());
    assert!(stopped.recording_state.is_none());
    session.observe_black_frame(black(true), u64::MAX);
    session.observe_watch(WatchTransition::Paused, u64::MAX);
    session.process_match(now, level_match(ge_cv::Screen::Start, 1, 2));
    drop(session);
    assert_eq!(snapshot.current(), stopped);
}

#[test]
fn observations_before_start_do_not_publish_or_prime_a_run() {
    let snapshot = snapshot_store();
    let initial = snapshot.current();
    let mut session = run_session(snapshot.clone());
    session.process_match(Instant::now(), level_match(ge_cv::Screen::Start, 1, 2));
    session.observe_black_frame(black(true), 100);
    session.observe_watch(WatchTransition::Paused, 200);
    assert_eq!(snapshot.current(), initial);
    session.start("N64 Capture".to_owned(), "en".to_owned());
    assert_eq!(snapshot.current().monitor.wall_clocks.level_timer_phase, LevelTimerPhase::Idle);
}

#[test]
fn unwinding_a_session_still_publishes_stopped_state() {
    let snapshot = snapshot_store();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut session = run_session(snapshot.clone());
        session.start("N64 Capture".to_owned(), "en".to_owned());
        panic!("simulated worker failure");
    }));
    assert!(result.is_err());
    assert!(!snapshot.current().monitor.enabled);
    assert!(!snapshot.current().monitor.wall_clocks.session_running);
}

#[test]
fn recording_votes_on_raw_frames_independently_of_the_smoothed_display() {
    let snapshot = snapshot_store();
    let mut session = run_session(snapshot.clone());
    session.start("N64 Capture".to_owned(), "en".to_owned());
    let now = Instant::now();
    session.process_match(now, level_match(ge_cv::Screen::Start, 1, 2));
    for time in std::iter::repeat_n(61, 10).chain(std::iter::repeat_n(60, 7)) {
        let mut matched = level_match(ge_cv::Screen::Stats, 1, 2);
        matched.times = Some(ge_game::Times { time, target_time: None, best_time: None });
        session.process_match(now, matched);
    }
    assert_eq!(snapshot.current().level_match.unwrap().times.unwrap().time, 60);
    let job = session.recording.as_mut().unwrap().take_pending_job(now).unwrap();
    assert_eq!(job.stats.unwrap().times.unwrap().time, 61);
}
