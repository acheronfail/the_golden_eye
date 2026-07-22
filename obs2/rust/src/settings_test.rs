use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

use serde_json::json;

use super::*;

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(label: &str) -> Self {
        loop {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!("ge-settings-{label}-{}-{nanos}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return TestDir { path },
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create test dir {}: {err}", path.display()),
            }
        }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn default_settings_use_five_second_pre_run_padding() {
    assert_eq!(AppSettings::default().pre_run_padding_secs, DEFAULT_PRE_RUN_PADDING_SECS);
    assert_eq!(AppSettings::default().failed_run_limit, DEFAULT_FAILED_RUN_LIMIT);
    assert_eq!(AppSettings::default().minimum_failed_run_length_secs, DEFAULT_MINIMUM_FAILED_RUN_LENGTH_SECS);
    assert!(!AppSettings::default().stop_replay_buffer_when_monitor_stopped);
    assert_eq!(AppSettings::default().monitor_design, DEFAULT_MONITOR_DESIGN);
    assert!(!AppSettings::default().show_monitor_fps);
    assert!(!AppSettings::default().show_developer_settings);
    assert!(AppSettings::default().show_source_previews);
    assert!(!AppSettings::default().welcome_modal_shown);
    assert_eq!(AppSettings::default().update_check_interval, UpdateCheckInterval::Weekly);
    assert_eq!(AppSettings::default().last_update_check_time, None);
    assert_eq!(AppSettings::default().youtube_visibility, DEFAULT_YOUTUBE_VISIBILITY);
    assert_eq!(AppSettings::default().youtube_title_template, DEFAULT_YOUTUBE_TITLE_TEMPLATE);
    assert_eq!(AppSettings::default().youtube_description_template, DEFAULT_YOUTUBE_DESCRIPTION_TEMPLATE);
    assert_eq!(AppSettings::from_json_value(json!({})).pre_run_padding_secs, DEFAULT_PRE_RUN_PADDING_SECS);
    assert_eq!(AppSettings::from_json_value(json!({})).failed_run_limit, DEFAULT_FAILED_RUN_LIMIT);
    assert_eq!(
        AppSettings::from_json_value(json!({})).minimum_failed_run_length_secs,
        DEFAULT_MINIMUM_FAILED_RUN_LENGTH_SECS
    );
    assert!(!AppSettings::from_json_value(json!({})).stop_replay_buffer_when_monitor_stopped);
    assert_eq!(AppSettings::from_json_value(json!({})).monitor_design, DEFAULT_MONITOR_DESIGN);
    assert_eq!(AppSettings::from_json_value(json!({ "monitorDesign": "debug" })).monitor_design, MonitorDesign::Debug);
    assert!(!AppSettings::from_json_value(json!({})).show_monitor_fps);
    assert!(!AppSettings::from_json_value(json!({})).show_developer_settings);
    assert!(AppSettings::from_json_value(json!({})).show_source_previews);
    assert!(!AppSettings::from_json_value(json!({})).welcome_modal_shown);
    assert_eq!(AppSettings::from_json_value(json!({})).update_check_interval, UpdateCheckInterval::Weekly);
    assert_eq!(AppSettings::from_json_value(json!({})).last_update_check_time, None);
    assert_eq!(AppSettings::from_json_value(json!({})).youtube_visibility, DEFAULT_YOUTUBE_VISIBILITY);
    assert_eq!(AppSettings::from_json_value(json!({})).youtube_title_template, DEFAULT_YOUTUBE_TITLE_TEMPLATE);
    assert_eq!(
        AppSettings::from_json_value(json!({})).youtube_description_template,
        DEFAULT_YOUTUBE_DESCRIPTION_TEMPLATE
    );
}

#[test]
fn json_value_is_normalized_field_by_field() {
    let settings = AppSettings::from_json_value(json!({
        "stopReplayBufferWhenMonitorStopped": true,
        "monitorDesign": "mission-glass",
        "showMonitorFps": true,
        "showDeveloperSettings": true,
        "showSourcePreviews": false,
        "welcomeModalShown": true,
        "completedOutputPath": "/tmp/completed",
        "saveFailedRuns": false,
        "failedOutputPath": "/tmp/failed",
        "failedRunLimit": "7.9",
        "minimumFailedRunLengthSecs": "20.5",
        "clipFilenameTemplate": LEGACY_CLIP_FILENAME_TEMPLATE,
        "preRunPaddingSecs": -3,
        "postRunPaddingSecs": "2.5",
        "discordNotificationsEnabled": false,
        "discordWebhookUrl": " https://discord.example/webhook ",
        "streamingStartedMessageTemplate": "",
        "streamingStoppedMessageTemplate": "Stopped {broadcast_url}",
        "updateCheckInterval": "daily",
        "lastUpdateCheckTime": "1234.9",
        "youtubeVisibility": "private",
        "youtubeTitleTemplate": "{level} PB",
        "youtubeDescriptionTemplate": "{time}"
    }));

    assert!(settings.stop_replay_buffer_when_monitor_stopped);
    assert_eq!(settings.monitor_design, MonitorDesign::MissionGlass);
    assert!(settings.show_monitor_fps);
    assert!(settings.show_developer_settings);
    assert!(!settings.show_source_previews);
    assert!(settings.welcome_modal_shown);
    assert_eq!(settings.completed_output_path, "/tmp/completed");
    assert!(!settings.save_failed_runs);
    assert_eq!(settings.failed_output_path, "/tmp/failed");
    assert_eq!(settings.failed_run_limit, 7);
    assert_eq!(settings.minimum_failed_run_length_secs, 20.5);
    assert_eq!(settings.clip_filename_template, DEFAULT_CLIP_FILENAME_TEMPLATE);
    assert_eq!(settings.pre_run_padding_secs, 0.0);
    assert_eq!(settings.post_run_padding_secs, 2.5);
    assert!(!settings.discord_notifications_enabled);
    assert_eq!(settings.discord_webhook_url, " https://discord.example/webhook ");
    assert_eq!(settings.streaming_started_message_template, DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE);
    assert_eq!(settings.streaming_stopped_message_template, "Stopped {broadcast_url}");
    assert_eq!(settings.update_check_interval, UpdateCheckInterval::Daily);
    assert_eq!(settings.last_update_check_time, Some(1234));

    let notification_options = settings.notification_options();
    assert!(!notification_options.enabled);
    assert_eq!(notification_options.discord_webhook_url, "https://discord.example/webhook");
}

#[test]
fn output_path_defaults_follow_obs_replay_directory_and_completed_path() {
    let replay_dir = PathBuf::from("/tmp/obs-replays");
    let settings = AppSettings::from_json_value(json!({})).with_default_output_paths(Some(&replay_dir));

    let default_completed = replay_dir.join("GoldenEye");
    assert_eq!(settings.completed_output_path, default_completed.to_string_lossy());
    assert_eq!(settings.failed_output_path, replay_dir.join("GoldenEye - failed").to_string_lossy());

    let custom_completed =
        AppSettings::from_json_value(json!({ "completedOutputPath": "/runs" })).with_default_output_paths(None);
    assert_eq!(custom_completed.completed_output_path, "/runs");
    assert_eq!(custom_completed.failed_output_path, Path::new("/runs - failed").to_string_lossy());
}

#[test]
fn status_includes_backend_defaults() {
    let dir = TestDir::new("status-defaults");
    let store = SettingsStore::load_from_path(dir.join("settings.json"));
    let status = store.status();

    assert_eq!(status.defaults.failed_run_limit, DEFAULT_FAILED_RUN_LIMIT);
    assert_eq!(status.defaults.minimum_failed_run_length_secs, DEFAULT_MINIMUM_FAILED_RUN_LENGTH_SECS);
}

#[test]
fn store_persists_and_loads_settings_json() {
    let dir = TestDir::new("persist");
    let path = dir.join("nested/settings.json");
    let store = SettingsStore::load_from_path(path.clone());

    let saved = store
        .replace(AppSettings::from_json_value(json!({
            "stopReplayBufferWhenMonitorStopped": true,
            "monitorDesign": "mission-glass",
            "showMonitorFps": true,
            "showDeveloperSettings": true,
            "showSourcePreviews": false,
            "welcomeModalShown": true,
            "completedOutputPath": "/runs",
            "saveFailedRuns": true,
            "failedOutputPath": "/fails",
            "failedRunLimit": 3,
            "minimumFailedRunLengthSecs": 12.5,
            "clipFilenameTemplate": "{level}",
            "preRunPaddingSecs": 1.25,
            "postRunPaddingSecs": 4,
            "discordNotificationsEnabled": false,
            "discordWebhookUrl": "https://discord.example/webhook",
            "streamingStartedMessageTemplate": "Started {broadcast_url}",
            "streamingStoppedMessageTemplate": "Stopped {broadcast_url}",
            "updateCheckInterval": "monthly",
            "lastUpdateCheckTime": 456
        })))
        .unwrap();

    assert_eq!(saved.completed_output_path, "/runs");
    assert!(saved.stop_replay_buffer_when_monitor_stopped);
    assert_eq!(saved.monitor_design, MonitorDesign::MissionGlass);
    assert!(saved.show_monitor_fps);
    assert!(saved.show_developer_settings);
    assert!(!saved.show_source_previews);
    assert!(saved.welcome_modal_shown);
    assert_eq!(saved.update_check_interval, UpdateCheckInterval::Monthly);
    assert_eq!(saved.last_update_check_time, Some(456));
    assert!(path.exists());

    let reloaded = SettingsStore::load_from_path(path).get();
    assert_eq!(reloaded, saved);
}

#[test]
fn store_updates_last_update_check_time_without_changing_other_settings() {
    let dir = TestDir::new("update-time");
    let store = SettingsStore::load_from_path(dir.join("settings.json"));
    store
        .replace(AppSettings::from_json_value(json!({
            "showMonitorFps": true,
            "updateCheckInterval": "daily"
        })))
        .unwrap();

    let saved = store.set_last_update_check_time(99).unwrap();

    assert!(saved.show_monitor_fps);
    assert_eq!(saved.update_check_interval, UpdateCheckInterval::Daily);
    assert_eq!(saved.last_update_check_time, Some(99));
}

#[test]
fn store_updates_last_known_update_without_changing_other_settings() {
    let dir = TestDir::new("update-known");
    let store = SettingsStore::load_from_path(dir.join("settings.json"));
    store
        .replace(AppSettings::from_json_value(json!({
            "showMonitorFps": true,
            "updateCheckInterval": "daily"
        })))
        .unwrap();

    let saved = store
        .set_last_known_update("v1.2.3", "https://github.com/acheronfail/the_golden_eye/releases/tag/v1.2.3")
        .unwrap();

    assert!(saved.show_monitor_fps);
    assert_eq!(saved.update_check_interval, UpdateCheckInterval::Daily);
    assert_eq!(saved.last_known_update_version, Some("v1.2.3".to_owned()));
    assert_eq!(
        saved.last_known_update_release_url,
        Some("https://github.com/acheronfail/the_golden_eye/releases/tag/v1.2.3".to_owned())
    );
}

#[test]
fn put_settings_preserves_backend_owned_last_known_update() {
    let dir = TestDir::new("preserve-known-update");
    let store = SettingsStore::load_from_path(dir.join("settings.json"));
    store.set_last_known_update("v1.2.3", "https://github.com/acheronfail/the_golden_eye/releases/tag/v1.2.3").unwrap();

    let saved = store
        .set_from_json_value_with_runtime_defaults(json!({
            "showMonitorFps": true,
            "updateCheckInterval": "daily",
            "lastKnownUpdateVersion": null,
            "lastKnownUpdateReleaseUrl": null
        }))
        .unwrap();

    assert!(saved.show_monitor_fps);
    assert_eq!(saved.update_check_interval, UpdateCheckInterval::Daily);
    assert_eq!(saved.last_known_update_version, Some("v1.2.3".to_owned()));
    assert_eq!(
        saved.last_known_update_release_url,
        Some("https://github.com/acheronfail/the_golden_eye/releases/tag/v1.2.3".to_owned())
    );
}

#[test]
fn put_settings_preserves_backend_owned_last_update_check_time() {
    let dir = TestDir::new("preserve-update-time");
    let store = SettingsStore::load_from_path(dir.join("settings.json"));
    store.set_last_update_check_time(99).unwrap();

    let saved = store
        .set_from_json_value_with_runtime_defaults(json!({
            "showMonitorFps": true,
            "updateCheckInterval": "daily",
            "lastUpdateCheckTime": null
        }))
        .unwrap();

    assert!(saved.show_monitor_fps);
    assert_eq!(saved.update_check_interval, UpdateCheckInterval::Daily);
    assert_eq!(saved.last_update_check_time, Some(99));
}
