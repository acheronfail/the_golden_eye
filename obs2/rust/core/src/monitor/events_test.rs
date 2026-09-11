use super::*;
use crate::http::MonitorWallClockState;
use crate::in_game_timer::LevelTimerPhase;

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

fn snapshot_store() -> SharedStateStore {
    SharedStateStore::new(crate::http::AppSnapshot {
        monitor: crate::http::MonitorSnapshot {
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
        replay_buffer: crate::http::ReplayBufferStatus::unknown(),
        settings_status: crate::settings::SettingsStatus {
            settings: crate::settings::AppSettings::default(),
            defaults: crate::settings::AppSettings::default(),
            config_path: "/tmp/settings.json".to_owned(),
            plugin_version: "test".to_owned(),
            file_error: None,
        },
        update: crate::updates::UpdateStatus::default(),
    })
}

fn event_handler(snapshot: SharedStateStore) -> MonitorEvents {
    let (tx, _) = tokio::sync::broadcast::channel(8);
    let recording = crate::recording::RecordingState::new(
        tx,
        crate::recording::RecordingStateStore::new(snapshot.clone()),
        crate::http::ReplaySaveStateStore::new(snapshot.clone()),
        crate::recording::RecordingOptions::default(),
        crate::recording::RecordingSessionContext::new("N64 Capture".to_owned(), "en".to_owned(), None),
        crate::recording::test_support::test_run_catalog("monitor-events"),
    );
    MonitorEvents::new(snapshot, recording)
}

fn started() -> MonitorEvent<'static> {
    MonitorEvent::SessionStarted { source_name: "N64 Capture".to_owned(), language: "en".to_owned() }
}

fn black_frame(detected: bool) -> MonitorEvent<'static> {
    MonitorEvent::BlackFrameObserved(BlackFrameSignal {
        detected,
        mean_luma: 0,
        dark_pixel_percent: 100,
        sample_count: 576,
        sample_region: crate::cv::ActivePictureRegion::full(640, 480),
    })
}

#[test]
fn events_publish_start_match_fades_pause_resume_and_stop_in_order() {
    let snapshot = snapshot_store();
    let mut events = event_handler(snapshot.clone());
    events.handle(started(), 1_000);
    assert!(snapshot.current().monitor.enabled);
    assert_eq!(snapshot.current().monitor.wall_clocks.session_started_at_unix_ms, Some(1_000));

    events.handle(MonitorEvent::DisplayMatchObserved(level_match(crate::cv::Screen::Start, 1, 2)), 1_100);
    let matched = snapshot.current();
    assert_eq!(matched.level_match.unwrap().screen, crate::cv::Screen::Start);
    assert_eq!(matched.monitor.wall_clocks.level_timer_phase, LevelTimerPhase::AwaitingInitialBlack);
    assert_eq!(matched.monitor.wall_clocks.intro_swirl_delay_ms, Some(3_167));

    for (at, black) in [(1_200, true), (1_300, false), (2_000, true), (2_100, false), (5_300, false)] {
        events.handle(black_frame(black), at);
    }
    assert_eq!(snapshot.current().monitor.wall_clocks.level_started_at_unix_ms, Some(5_267));
    events.handle(MonitorEvent::WatchChanged(WatchTransition::Paused), 6_000);
    let paused = snapshot.current().monitor.wall_clocks;
    assert!(paused.level_paused);
    assert!(!paused.level_running);
    assert_eq!(paused.level_elapsed_ms, 733);

    events.handle(MonitorEvent::WatchChanged(WatchTransition::Resumed), 7_000);
    assert!(snapshot.current().monitor.wall_clocks.level_running);
    events.handle(MonitorEvent::SessionStopping, 0);
    events.handle(MonitorEvent::SessionStopped, 8_000);
    let stopped = snapshot.current();
    assert!(!stopped.monitor.enabled);
    assert!(stopped.level_match.is_none());
    assert_eq!(stopped.monitor.wall_clocks.session_elapsed_ms, 7_000);
    assert_eq!(stopped.monitor.wall_clocks.level_elapsed_ms, 1_733);
    assert_eq!(stopped.monitor.wall_clocks.level_timer_phase, LevelTimerPhase::Stopped);
    drop(events);
    assert_eq!(snapshot.current(), stopped, "dropping an explicitly stopped owner must not republish");
}

#[test]
fn observations_outside_a_session_do_not_publish_or_prime_the_next_session() {
    let snapshot = snapshot_store();
    let mut events = event_handler(snapshot.clone());
    let initial = snapshot.current();
    events.handle(MonitorEvent::DisplayMatchObserved(level_match(crate::cv::Screen::Start, 1, 2)), 100);
    events.handle(black_frame(true), 200);
    events.handle(MonitorEvent::WatchChanged(WatchTransition::Paused), 300);
    assert_eq!(snapshot.current(), initial);

    events.handle(started(), 1_000);
    assert_eq!(snapshot.current().monitor.wall_clocks.level_timer_phase, LevelTimerPhase::Idle);
    events.handle(MonitorEvent::SessionStopping, 0);
    events.handle(MonitorEvent::SessionStopped, 2_000);
    let stopped = snapshot.current();
    events.handle(black_frame(true), 3_000);
    events.handle(MonitorEvent::WatchChanged(WatchTransition::Resumed), 4_000);
    events.handle(MonitorEvent::DisplayMatchObserved(level_match(crate::cv::Screen::Start, 1, 2)), 5_000);
    assert_eq!(snapshot.current(), stopped);
}

#[test]
fn worker_unwinding_still_publishes_a_stopped_snapshot() {
    let snapshot = snapshot_store();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut events = event_handler(snapshot.clone());
        events.handle(started(), 1_000);
        panic!("simulated monitor failure");
    }));
    assert!(result.is_err());
    assert!(!snapshot.current().monitor.enabled);
    assert!(!snapshot.current().monitor.wall_clocks.session_running);
    assert_eq!(snapshot.current().monitor.wall_clocks.level_timer_phase, LevelTimerPhase::Stopped);
}

#[test]
fn raw_matches_drive_recording_while_display_matches_only_update_clocks() {
    use crate::recording::RecordingStatus;
    let snapshot = snapshot_store();
    let mut events = event_handler(snapshot.clone());
    events.handle(started(), 1_000);
    let now = Instant::now();
    let raw_start = level_match(crate::cv::Screen::Start, 1, 2);
    events.handle(MonitorEvent::RawMatchObserved { matched: &raw_start, now }, 1_100);
    assert_eq!(snapshot.current().recording_state, Some(RecordingStatus::Started));
    assert!(snapshot.current().level_match.is_none());
    assert_eq!(snapshot.current().monitor.wall_clocks.level_timer_phase, LevelTimerPhase::Idle);

    events.handle(MonitorEvent::DisplayMatchObserved(raw_start), 1_200);
    assert_eq!(snapshot.current().monitor.wall_clocks.level_timer_phase, LevelTimerPhase::AwaitingInitialBlack);
    events.handle(MonitorEvent::DisplayMatchObserved(level_match(crate::cv::Screen::Stats, 1, 2)), 1_300);
    assert_eq!(snapshot.current().recording_state, Some(RecordingStatus::Started));
    assert!(events.pending_fire_at().is_none(), "display matches must never schedule a replay save");
    events.handle(MonitorEvent::SaveDeadlineReached(now), 1_400);
    assert_eq!(snapshot.current().recording_state, Some(RecordingStatus::Started));

    events.handle(MonitorEvent::LanguageChanged("jp".to_owned()), 1_500);
    assert_eq!(snapshot.current().monitor.cv_language.as_deref(), Some("jp"));
    events.handle(MonitorEvent::SessionStopping, 2_000);
    assert!(snapshot.current().monitor.wall_clocks.session_running);
    events.handle(MonitorEvent::SessionStopped, 3_000);
    assert_eq!(snapshot.current().monitor.wall_clocks.session_elapsed_ms, 2_000);
    assert!(snapshot.current().recording_state.is_none());
}
