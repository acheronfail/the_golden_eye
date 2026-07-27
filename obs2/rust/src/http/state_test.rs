use super::*;

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
fn monitor_version_event_uses_frontend_field_name() {
    let event = AppEvent::Version { build_id: "abc123".to_owned() };
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "version");
    assert_eq!(json["buildId"], "abc123");
    assert!(json.get("build_id").is_none());
}

fn test_snapshot() -> AppSnapshot {
    AppSnapshot {
        monitor: MonitorSnapshot {
            enabled: true,
            source_name: Some("N64 Capture".to_owned()),
            cv_language: Some("en".to_owned()),
            wall_clocks: MonitorWallClockState::default(),
        },
        level_match: None,
        run_catalog_sync: Some(RunCatalogSync::Initial),
        recording_state: Some(RecordingStatus::Started),
        replay_saves: vec![],
        sources: vec![routes::sources::Source { name: "N64 Capture".to_owned(), id: "av_capture_input".to_owned() }],
        replay_buffer: routes::record::ReplayBufferStatus {
            enabled: true,
            available: true,
            active: true,
            max_seconds: Some(1200),
            output_directory: Some("/captures".to_owned()),
            default_completed_output_path: Some("/captures/GoldenEye".to_owned()),
        },
        settings_status: crate::settings::SettingsStatus {
            settings: crate::settings::AppSettings::default(),
            defaults: crate::settings::AppSettings::default(),
            config_path: "/tmp/settings.json".to_owned(),
            plugin_version: "test".to_owned(),
            file_error: None,
        },
        update: crate::updates::UpdateStatus {
            phase: crate::updates::UpdatePhase::Available,
            available: Some(crate::updates::PluginUpdate {
                current_version: "1.0.0".to_owned(),
                latest_version: "1.1.0".to_owned(),
                release_url: "https://github.com/acheronfail/the_golden_eye/releases/tag/v1.1.0".to_owned(),
                updater_version: 0,
                requires_manual_install: false,
            }),
        },
    }
}

#[test]
fn snapshot_event_contains_retained_app_state() {
    let event = AppEvent::Snapshot { state: Box::new(test_snapshot()) };
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "snapshot");
    assert_eq!(json["state"]["monitor"]["enabled"], true);
    assert_eq!(json["state"]["monitor"]["sourceName"], "N64 Capture");
    assert_eq!(json["state"]["monitor"]["cvLanguage"], "en");
    assert_eq!(json["state"]["monitor"]["wallClocks"]["sessionElapsedMs"], 0);
    assert_eq!(json["state"]["monitor"]["wallClocks"]["levelRunning"], false);
    assert!(json["state"]["monitor"]["wallClocks"]["introSwirlDelayMs"].is_null());
    assert_eq!(json["state"]["monitor"]["wallClocks"]["levelTimerPhase"], "idle");
    assert!(json["state"]["match"].is_null());
    assert_eq!(json["state"]["runCatalogSync"], "initial");
    assert_eq!(json["state"]["recordingState"], "started");
    assert_eq!(json["state"]["replaySaves"], serde_json::json!([]));
    assert_eq!(json["state"]["sources"][0]["name"], "N64 Capture");
    assert_eq!(json["state"]["replayBuffer"]["active"], true);
    assert_eq!(json["state"]["settingsStatus"]["configPath"], "/tmp/settings.json");
    assert_eq!(json["state"]["update"]["phase"], "available");
    assert_eq!(json["state"]["update"]["available"]["latestVersion"], "1.1.0");
}

#[test]
fn monitor_wall_clocks_follow_backend_screen_transitions() {
    let mut clocks = MonitorWallClockState::default();

    clocks.start_session(1_000);
    clocks.reconcile_screen(crate::cv::Screen::Start, 1_100);
    clocks.reconcile_screen(crate::cv::Screen::Unknown, 1_250);
    assert_eq!(clocks.level_started_at_unix_ms, None);
    assert_eq!(clocks.level_elapsed_ms, 0);
    assert_eq!(clocks.level_timer_phase, LevelTimerPhase::AwaitingInitialBlack);
    assert!(!clocks.level_running);

    clocks.reconcile_screen(crate::cv::Screen::Stats, 3_750);
    assert_eq!(clocks.level_elapsed_ms, 0);
    assert_eq!(clocks.level_timer_phase, LevelTimerPhase::Stopped);
    assert!(!clocks.level_running);

    clocks.reconcile_screen(crate::cv::Screen::Unknown, 4_000);
    assert_eq!(clocks.level_elapsed_ms, 0);
    assert!(!clocks.level_running);

    clocks.stop_session(5_000);
    assert_eq!(clocks.session_elapsed_ms, 4_000);
    assert!(!clocks.session_running);
}

#[test]
fn monitor_wall_clock_starts_on_a_skipped_second_cutscene_and_stops_on_the_next_fade() {
    let mut clocks = MonitorWallClockState::default();
    let sample_region = crate::cv::ActivePictureRegion::full(640, 480);
    let black = crate::cv::BlackFrameSignal {
        detected: true,
        mean_luma: 14,
        dark_pixel_percent: 100,
        sample_count: 576,
        sample_region,
    };
    let visible = crate::cv::BlackFrameSignal {
        detected: false,
        mean_luma: 72,
        dark_pixel_percent: 11,
        sample_count: 576,
        sample_region,
    };

    clocks.start_session(1_000);
    clocks.reconcile_screen(crate::cv::Screen::Start, 1_100);
    clocks.reconcile_screen(crate::cv::Screen::Unknown, 1_200);
    clocks.reconcile_black_frame(black, 1_300);
    assert_eq!(clocks.level_timer_phase, LevelTimerPhase::AwaitingFirstCutscene);
    clocks.reconcile_black_frame(visible, 1_400);
    assert_eq!(clocks.level_timer_phase, LevelTimerPhase::AwaitingFirstCutsceneFade);
    clocks.reconcile_black_frame(black, 2_000);
    assert_eq!(clocks.level_timer_phase, LevelTimerPhase::AwaitingSecondFadeOrSwirl);
    clocks.reconcile_black_frame(visible, 2_100);
    clocks.reconcile_black_frame(black, 2_500);
    assert_eq!(clocks.level_timer_phase, LevelTimerPhase::AwaitingGameplayAfterSkip);
    assert!(!clocks.level_running);
    clocks.reconcile_black_frame(black, 2_800);
    assert_eq!(clocks.level_timer_phase, LevelTimerPhase::AwaitingGameplayAfterSkip);
    assert!(!clocks.level_running, "a sustained skip fade must not start the timer");
    clocks.reconcile_black_frame(visible, 3_000);
    assert_eq!(clocks.level_started_at_unix_ms, Some(2_800));
    assert_eq!(clocks.level_elapsed_ms, 200);
    assert_eq!(clocks.level_start_reason, Some(LevelTimerStartReason::Fade));
    assert_eq!(clocks.level_timer_phase, LevelTimerPhase::Running);
    assert_eq!(clocks.fade_detection, Some(visible));
    let json = serde_json::to_value(&clocks).unwrap();
    assert_eq!(json["levelStartReason"], "fade");
    assert_eq!(json["levelTimerPhase"], "running");
    assert_eq!(json["levelElapsedMs"], 200);
    assert_eq!(json["fadeDetection"]["meanLuma"], 72);
    assert_eq!(json["fadeDetection"]["darkPixelPercent"], 11);
    assert_eq!(json["fadeDetection"]["sampleRegion"]["width"], 640);

    clocks.reconcile_black_frame(black, 5_000);
    assert_eq!(clocks.level_elapsed_ms, 2_200);
    assert_eq!(clocks.level_timer_phase, LevelTimerPhase::Stopped);
    assert!(!clocks.level_running);
}

#[test]
fn monitor_wall_clock_starts_when_the_level_swirl_delay_expires() {
    let sample_region = crate::cv::ActivePictureRegion::full(640, 480);
    let black = crate::cv::BlackFrameSignal {
        detected: true,
        mean_luma: 30,
        dark_pixel_percent: 100,
        sample_count: 576,
        sample_region,
    };
    let visible = crate::cv::BlackFrameSignal { detected: false, mean_luma: 80, dark_pixel_percent: 5, ..black };

    let mut clocks = MonitorWallClockState::default();
    clocks.start_session(1_000);
    clocks.reconcile_match(&level_match(crate::cv::Screen::Start, 1, 2), 1_100);
    assert_eq!(clocks.intro_swirl_delay_ms, Some(3_167));
    clocks.reconcile_black_frame(black, 1_200);
    clocks.reconcile_black_frame(visible, 1_300);
    clocks.reconcile_black_frame(black, 2_000);
    clocks.reconcile_black_frame(visible, 2_100);

    clocks.reconcile_black_frame(visible, 5_266);
    assert!(!clocks.level_running);
    clocks.reconcile_black_frame(visible, 5_300);
    assert_eq!(clocks.level_started_at_unix_ms, Some(5_267));
    assert_eq!(clocks.level_elapsed_ms, 0);
    assert_eq!(clocks.level_start_reason, Some(LevelTimerStartReason::Swirl));
    assert_eq!(clocks.level_timer_phase, LevelTimerPhase::Running);
}

#[test]
fn first_black_frame_after_the_swirl_deadline_stops_the_level_timer() {
    let sample_region = crate::cv::ActivePictureRegion::full(640, 480);
    let black = crate::cv::BlackFrameSignal {
        detected: true,
        mean_luma: 30,
        dark_pixel_percent: 100,
        sample_count: 576,
        sample_region,
    };
    let visible = crate::cv::BlackFrameSignal { detected: false, mean_luma: 80, dark_pixel_percent: 5, ..black };

    let mut clocks = MonitorWallClockState::default();
    clocks.start_session(1_000);
    clocks.reconcile_match(&level_match(crate::cv::Screen::Start, 1, 2), 1_100);
    clocks.reconcile_black_frame(black, 1_200);
    clocks.reconcile_black_frame(visible, 1_300);
    clocks.reconcile_black_frame(black, 2_000);
    clocks.reconcile_black_frame(visible, 2_100);

    clocks.reconcile_black_frame(black, 5_300);
    assert_eq!(clocks.level_start_reason, Some(LevelTimerStartReason::Swirl));
    assert_eq!(clocks.level_elapsed_ms, 33);
    assert_eq!(clocks.level_timer_phase, LevelTimerPhase::Stopped);
    assert!(!clocks.level_running);
}

#[test]
fn start_match_selects_the_level_intro_swirl_delay() {
    let mut clocks = MonitorWallClockState::default();

    clocks.start_session(1_000);
    clocks.reconcile_match(&level_match(crate::cv::Screen::Start, 7, 4), 1_100);

    assert_eq!(clocks.intro_swirl_delay_ms, Some(crate::ge::intro::swirl_delay_ms(crate::ge::Level::Cradle)));
    assert_eq!(serde_json::to_value(&clocks).unwrap()["introSwirlDelayMs"], 4_567);

    clocks.reconcile_match(&level_match(crate::cv::Screen::Start, -1, -1), 1_200);
    assert_eq!(clocks.intro_swirl_delay_ms, None);
}

#[test]
fn black_frame_diagnostics_update_immediately_for_edges_and_periodically_for_evidence() {
    let mut clocks = MonitorWallClockState::default();
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
    assert_eq!(clocks.fade_detection, Some(signal));

    signal.sample_region = crate::cv::ActivePictureRegion { x: 107, y: 0, width: 640, height: 480 };
    assert!(clocks.reconcile_black_frame(signal, 1_300));
    assert_eq!(clocks.fade_detection, Some(signal));
}

#[test]
fn monitor_wall_clocks_reset_on_the_next_start_screen() {
    let mut clocks = MonitorWallClockState::default();

    clocks.start_session(1_000);
    clocks.start_level(1_200, LevelTimerStartReason::Fade);
    clocks.reconcile_screen(crate::cv::Screen::Stats, 2_200);
    assert_eq!(clocks.level_elapsed_ms, 1_000);

    clocks.reconcile_screen(crate::cv::Screen::Start, 3_000);
    assert_eq!(clocks.level_elapsed_ms, 0);
    assert_eq!(clocks.level_timer_phase, LevelTimerPhase::AwaitingInitialBlack);
    assert!(!clocks.level_running);
}

#[test]
fn youtube_status_changed_event_uses_frontend_field_names() {
    let event = AppEvent::YoutubeStatusChanged {
        status: crate::youtube::YoutubeStatus {
            enabled: true,
            oauth_configured: true,
            connected: true,
            account: None,
            uploads: vec![],
            history: vec![],
        },
    };
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "youtubeStatusChanged");
    assert_eq!(json["status"]["oauthConfigured"], true);
    assert_eq!(json["status"]["connected"], true);
    assert!(json["status"].get("oauth_configured").is_none());
}

#[test]
fn monitor_fps_event_uses_frontend_field_names() {
    let event = AppEvent::MonitorFps(MonitorFps {
        processed_fps: 59.5,
        captured_fps: 60.0,
        source_fps: 60.0,
        dropped_frames: 1,
        health: MonitorFpsHealth::Warning,
    });
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "monitorFps");
    assert_eq!(json["processedFps"], 59.5);
    assert_eq!(json["capturedFps"], 60.0);
    assert_eq!(json["sourceFps"], 60.0);
    assert_eq!(json["droppedFrames"], 1);
    assert_eq!(json["health"], "warning");
    assert!(json.get("processed_fps").is_none());
}

#[test]
fn recording_save_pending_event_uses_frontend_field_names() {
    let event = AppEvent::RecordingSavePending(RecordingSavePending {
        save_id: 7,
        save_in_secs: 5.0,
        estimated_duration_secs: 74.5,
        failed: false,
        status: "complete".to_owned(),
        level: "Dam".to_owned(),
        level_number: Some(1),
        difficulty: Some("Agent".to_owned()),
        time_secs: Some(69),
        target_time_secs: Some(120),
        best_time_secs: None,
        stats: None,
    });
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "recordingSavePending");
    assert_eq!(json["saveId"], 7);
    assert_eq!(json["saveInSecs"], 5.0);
    assert_eq!(json["estimatedDurationSecs"], 74.5);
    assert_eq!(json["timeSecs"], 69);
    assert!(json.get("bestTimeSecs").is_none());
}

#[test]
fn recording_saved_event_uses_frontend_field_names() {
    let event = AppEvent::RecordingSaved(RecordingSaved {
        save_id: 7,
        path: "/tmp/clip.mp4".to_owned(),
        replay_path: "/tmp/replay.mp4".to_owned(),
        duration_secs: 74.5,
        failed: false,
        stats: None,
    });
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "recordingSaved");
    assert_eq!(json["saveId"], 7);
    assert_eq!(json["path"], "/tmp/clip.mp4");
    assert_eq!(json["replayPath"], "/tmp/replay.mp4");
    assert_eq!(json["durationSecs"], 74.5);
    assert!(json.get("stats").is_none());
}

#[test]
fn run_catalog_changed_links_a_finalized_run_to_its_pending_save() {
    let event = AppEvent::RunCatalogChanged { run_id: Some("run-7".to_owned()), save_id: Some(7) };
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "runCatalogChanged");
    assert_eq!(json["runId"], "run-7");
    assert_eq!(json["saveId"], 7);
}

#[tokio::test]
async fn snapshot_store_does_not_notify_for_noop_writes() {
    let snapshot = SharedStateStore::new(test_snapshot());
    let mut rx = snapshot.subscribe();

    snapshot.set_sources(snapshot.current().sources);
    assert!(tokio::time::timeout(Duration::from_millis(10), rx.changed()).await.is_err());

    snapshot.set_monitor_stopped();
    assert!(tokio::time::timeout(Duration::from_millis(100), rx.changed()).await.unwrap().is_ok());

    snapshot.set_update_status(crate::updates::UpdateStatus {
        phase: crate::updates::UpdatePhase::Downloading,
        available: snapshot.current_update_status().available,
    });
    assert!(tokio::time::timeout(Duration::from_millis(100), rx.changed()).await.unwrap().is_ok());
    assert_eq!(snapshot.current_update_status().phase, crate::updates::UpdatePhase::Downloading);
}

#[test]
fn monitor_snapshot_tracks_and_clears_the_active_cv_language() {
    let snapshot = SharedStateStore::new(test_snapshot());

    snapshot.set_monitor_language("jp".to_owned());
    assert_eq!(snapshot.current().monitor.cv_language.as_deref(), Some("jp"));

    snapshot.set_monitor_stopped();
    assert_eq!(snapshot.current().monitor.cv_language, None);

    snapshot.set_monitor_running("N64 Capture".to_owned(), "en".to_owned());
    assert_eq!(snapshot.current().monitor.cv_language.as_deref(), Some("en"));
}

#[test]
fn replay_save_store_retains_pipeline_transitions() {
    let snapshot = SharedStateStore::new(test_snapshot());
    let store = ReplaySaveStateStore::new(snapshot.clone());
    store.schedule(ReplaySaveStatus {
        tracking_id: 41,
        save_id: 7,
        stage: ReplaySaveStage::Scheduled,
        level: "Facility".to_owned(),
        difficulty: Some("00 Agent".to_owned()),
        run_status: "complete".to_owned(),
        estimated_duration_secs: 68.0,
        error: None,
    });

    store.transition(41, ReplaySaveStage::SavingReplay);
    assert_eq!(snapshot.current().replay_saves[0].stage, ReplaySaveStage::SavingReplay);

    store.transition(41, ReplaySaveStage::Trimming);
    assert_eq!(snapshot.current().replay_saves[0].stage, ReplaySaveStage::Trimming);

    store.complete(41);
    assert_eq!(snapshot.current().replay_saves[0].stage, ReplaySaveStage::Completed);
}

#[test]
fn recording_state_store_updates_snapshot_without_receivers() {
    let snapshot = SharedStateStore::new(test_snapshot());
    let rx = snapshot.subscribe();
    let store = RecordingStateStore::new(snapshot.clone());
    drop(rx);

    store.set(RecordingStatus::Started);
    assert_eq!(store.current(), Some(RecordingStatus::Started));
    assert_eq!(snapshot.current().recording_state, Some(RecordingStatus::Started));

    // A stale generation (superseded by a later transition) must not clear
    // the phase, even though its captured value matches the current one.
    let stale_generation = store.set(RecordingStatus::SavePending);
    store.set(RecordingStatus::Started);
    store.clear_if_generation(stale_generation);
    assert_eq!(store.current(), Some(RecordingStatus::Started));

    // The current generation clears normally.
    let current_generation = store.set(RecordingStatus::SavePending);
    store.clear_if_generation(current_generation);
    assert_eq!(store.current(), None);

    store.set(RecordingStatus::Started);
    store.clear();
    assert_eq!(store.current(), None);
    assert_eq!(snapshot.current().recording_state, None);
}

#[test]
fn monitor_stopped_event_uses_frontend_field_names() {
    let event = AppEvent::MonitorStopped { reason: MonitorStoppedReason::ReplayBufferStopped };
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "monitorStopped");
    assert_eq!(json["reason"], "replayBufferStopped");

    let event = AppEvent::MonitorStopped { reason: MonitorStoppedReason::UserStopped };
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "monitorStopped");
    assert_eq!(json["reason"], "userStopped");
}
