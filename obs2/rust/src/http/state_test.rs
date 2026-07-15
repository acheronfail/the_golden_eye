use super::*;

#[test]
fn monitor_version_event_uses_frontend_field_name() {
    let event = MonitorEvent::Version { build_id: "abc123".to_owned() };
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "version");
    assert_eq!(json["buildId"], "abc123");
    assert!(json.get("build_id").is_none());
}

fn test_snapshot() -> AppSnapshot {
    AppSnapshot {
        monitor: MonitorSnapshot { enabled: true, source_name: Some("N64 Capture".to_owned()) },
        level_match: None,
        recording_state: Some(RecordingStatus::Started),
        sources: vec![routes::sources::Source { name: "N64 Capture".to_owned(), id: "av_capture_input".to_owned() }],
        replay_buffer: routes::record::ReplayBufferStatus {
            enabled: true,
            available: true,
            active: true,
            max_seconds: Some(1200),
            output_directory: Some("/captures".to_owned()),
            default_completed_output_path: Some("/captures/GoldenEye".to_owned()),
            default_failed_output_path: Some("/captures/GoldenEye/failed".to_owned()),
        },
        settings_status: crate::settings::SettingsStatus {
            settings: crate::settings::AppSettings::default(),
            defaults: crate::settings::AppSettings::default(),
            config_path: "/tmp/settings.json".to_owned(),
            file_error: None,
        },
        update: Some(crate::updates::PluginUpdate {
            current_version: "1.0.0".to_owned(),
            latest_version: "1.1.0".to_owned(),
            release_url: "https://github.com/acheronfail/the_golden_eye/releases/tag/v1.1.0".to_owned(),
        }),
    }
}

#[test]
fn snapshot_event_contains_retained_app_state() {
    let event = MonitorEvent::Snapshot { state: Box::new(test_snapshot()) };
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "snapshot");
    assert_eq!(json["state"]["monitor"]["enabled"], true);
    assert_eq!(json["state"]["monitor"]["sourceName"], "N64 Capture");
    assert!(json["state"]["match"].is_null());
    assert_eq!(json["state"]["recordingState"], "started");
    assert_eq!(json["state"]["sources"][0]["name"], "N64 Capture");
    assert_eq!(json["state"]["replayBuffer"]["active"], true);
    assert_eq!(json["state"]["settingsStatus"]["configPath"], "/tmp/settings.json");
    assert_eq!(json["state"]["update"]["latestVersion"], "1.1.0");
}

#[test]
fn language_detected_event_uses_frontend_field_names() {
    let event = MonitorEvent::LanguageDetected { lang: "en".to_owned() };
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "languageDetected");
    assert_eq!(json["lang"], "en");
}

#[test]
fn monitor_fps_event_uses_frontend_field_names() {
    let event = MonitorEvent::MonitorFps(MonitorFps { processed_fps: 59.5, source_fps: 60.0 });
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "monitorFps");
    assert_eq!(json["processedFps"], 59.5);
    assert_eq!(json["sourceFps"], 60.0);
    assert!(json.get("processed_fps").is_none());
}

#[test]
fn recording_save_pending_event_uses_frontend_field_names() {
    let event = MonitorEvent::RecordingSavePending(RecordingSavePending {
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
fn recording_save_discarded_event_uses_frontend_field_names() {
    let event = MonitorEvent::RecordingSaveDiscarded { save_id: 7 };
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "recordingSaveDiscarded");
    assert_eq!(json["saveId"], 7);
}

#[test]
fn recording_saved_event_uses_frontend_field_names() {
    let event = MonitorEvent::RecordingSaved(RecordingSaved {
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

#[tokio::test]
async fn snapshot_store_does_not_notify_for_noop_writes() {
    let snapshot = SharedStateStore::new(test_snapshot());
    let mut rx = snapshot.subscribe();

    snapshot.set_sources(snapshot.current().sources);
    assert!(tokio::time::timeout(Duration::from_millis(10), rx.changed()).await.is_err());

    snapshot.set_monitor_stopped();
    assert!(tokio::time::timeout(Duration::from_millis(100), rx.changed()).await.unwrap().is_ok());
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
    let event = MonitorEvent::MonitorStopped { reason: MonitorStoppedReason::ReplayBufferStopped };
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "monitorStopped");
    assert_eq!(json["reason"], "replayBufferStopped");

    let event = MonitorEvent::MonitorStopped { reason: MonitorStoppedReason::UserStopped };
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "monitorStopped");
    assert_eq!(json["reason"], "userStopped");
}
