use std::time::Duration;

use super::*;
use crate::app::AppEvent;

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
        sources: vec![Source { name: "N64 Capture".to_owned(), id: "av_capture_input".to_owned() }],
        replay_buffer: ReplayBufferStatus {
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
        update: crate::plugin_updates::UpdateStatus {
            phase: crate::plugin_updates::UpdatePhase::Available,
            available: Some(crate::plugin_updates::PluginUpdate {
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
    assert_eq!(json["state"]["monitor"]["wallClocks"]["levelPaused"], false);
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
fn youtube_status_changed_event_uses_frontend_field_names() {
    let event = AppEvent::YoutubeStatusChanged {
        status: crate::youtube_uploads::YoutubeStatus {
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

    crate::run_monitoring::publication::MonitorPublisher::new(snapshot.clone())
        .set_monitor_stopped(MonitorWallClockState::default());
    assert!(tokio::time::timeout(Duration::from_millis(100), rx.changed()).await.unwrap().is_ok());

    let available = snapshot.current_update_status().available;
    snapshot.update(|state| {
        state.update =
            crate::plugin_updates::UpdateStatus { phase: crate::plugin_updates::UpdatePhase::Downloading, available }
    });
    assert!(tokio::time::timeout(Duration::from_millis(100), rx.changed()).await.unwrap().is_ok());
    assert_eq!(snapshot.current_update_status().phase, crate::plugin_updates::UpdatePhase::Downloading);
}

#[test]
fn monitor_snapshot_tracks_and_clears_the_active_cv_language() {
    let snapshot = SharedStateStore::new(test_snapshot());

    crate::run_monitoring::publication::MonitorPublisher::new(snapshot.clone()).set_monitor_language("jp".to_owned());
    assert_eq!(snapshot.current().monitor.cv_language.as_deref(), Some("jp"));

    crate::run_monitoring::publication::MonitorPublisher::new(snapshot.clone())
        .set_monitor_stopped(MonitorWallClockState::default());
    assert_eq!(snapshot.current().monitor.cv_language, None);

    crate::run_monitoring::publication::MonitorPublisher::new(snapshot.clone()).set_monitor_running(
        "N64 Capture".to_owned(),
        "en".to_owned(),
        MonitorWallClockState::default(),
    );
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

#[test]
fn publishing_a_match_does_not_infer_timer_transitions() {
    let snapshot = SharedStateStore::new(test_snapshot());
    let clocks = MonitorWallClockState {
        level_elapsed_ms: 321,
        level_timer_phase: LevelTimerPhase::Stopped,
        ..MonitorWallClockState::default()
    };
    let level_match = LevelMatch {
        screen: ge_cv::Screen::Start,
        mission: 1,
        part: 2,
        difficulty: 0,
        detected_lang: None,
        times: None,
        raw_times: vec![],
        match_regions: vec![],
        annotation_sets: vec![],
        runtime_ms: 0.0,
    };
    crate::run_monitoring::publication::MonitorPublisher::new(snapshot.clone())
        .set_match(Some(level_match), clocks.clone());
    assert_eq!(snapshot.current().monitor.wall_clocks, clocks);
}

#[test]
fn concurrent_publications_do_not_lose_updates_or_move_backwards() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    let snapshot = SharedStateStore::new(test_snapshot());
    let updates = 500;
    let writers = 4;
    let barrier = Arc::new(Barrier::new(writers + 1));
    let finished = Arc::new(AtomicUsize::new(0));
    std::thread::scope(|scope| {
        for _ in 0..writers {
            let snapshot = snapshot.clone();
            let barrier = barrier.clone();
            let finished = finished.clone();
            scope.spawn(move || {
                barrier.wait();
                for _ in 0..updates {
                    snapshot.update(|state| state.monitor.wall_clocks.session_elapsed_ms += 1);
                    std::thread::yield_now();
                }
                finished.fetch_add(1, Ordering::Release);
            });
        }
        let rx = snapshot.subscribe();
        barrier.wait();
        let mut previous = 0;
        while finished.load(Ordering::Acquire) != writers {
            let elapsed = rx.borrow().monitor.wall_clocks.session_elapsed_ms;
            assert!(elapsed >= previous, "published state moved backwards");
            previous = elapsed;
            std::thread::yield_now();
        }
        assert_eq!(rx.borrow().monitor.wall_clocks.session_elapsed_ms, (updates * writers) as u64);
    });
}
