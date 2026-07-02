//! Persisted application settings.
//!
//! These are user-editable options shared between the SPA and Rust runtime. The
//! JSON file is intentionally owned by Rust so OBS-triggered workflows can
//! read the same configuration even when no browser tab is open.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;
use serde::Serialize;
use serde_json::Value;

use crate::recording::{
    DEFAULT_CLIP_FILENAME_TEMPLATE, DEFAULT_POST_RUN_PADDING_SECS, DEFAULT_PRE_RUN_PADDING_SECS, RecordingOptions,
};
use crate::stream_notifier::{DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE, DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE};

const SETTINGS_FILE_NAME: &str = "settings.json";
const LEGACY_CLIP_FILENAME_TEMPLATE: &str = "{replay} - clip - {level}{time_suffix}{failed_suffix}";

/// User settings stored in the plugin-owned JSON file and mirrored by the SPA's
/// bindable settings object.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub open_golden_eye_on_launch: bool,
    pub stop_replay_buffer_when_monitor_stopped: bool,
    pub developer_lang: String,
    pub completed_output_path: String,
    pub save_failed_runs: bool,
    pub failed_output_path: String,
    pub failed_run_limit: usize,
    pub clip_filename_template: String,
    pub pre_run_padding_secs: f64,
    pub post_run_padding_secs: f64,
    pub discord_notifications_enabled: bool,
    pub discord_webhook_url: String,
    pub streaming_started_message_template: String,
    pub streaming_stopped_message_template: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            open_golden_eye_on_launch: true,
            stop_replay_buffer_when_monitor_stopped: false,
            developer_lang: "en".to_owned(),
            completed_output_path: String::new(),
            save_failed_runs: true,
            failed_output_path: String::new(),
            failed_run_limit: 0,
            clip_filename_template: DEFAULT_CLIP_FILENAME_TEMPLATE.to_owned(),
            pre_run_padding_secs: DEFAULT_PRE_RUN_PADDING_SECS,
            post_run_padding_secs: DEFAULT_POST_RUN_PADDING_SECS,
            discord_notifications_enabled: true,
            discord_webhook_url: String::new(),
            streaming_started_message_template: DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE.to_owned(),
            streaming_stopped_message_template: DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE.to_owned(),
        }
    }
}

impl AppSettings {
    pub fn from_json_value(value: Value) -> Self {
        let default = AppSettings::default();
        let Some(object) = value.as_object() else {
            return default;
        };

        Self {
            open_golden_eye_on_launch: bool_field(
                object.get("openGoldenEyeOnLaunch"),
                default.open_golden_eye_on_launch,
            ),
            stop_replay_buffer_when_monitor_stopped: bool_field(
                object.get("stopReplayBufferWhenMonitorStopped"),
                default.stop_replay_buffer_when_monitor_stopped,
            ),
            developer_lang: developer_lang(object.get("developerLang"), &default.developer_lang),
            completed_output_path: string_field(object.get("completedOutputPath"), &default.completed_output_path),
            save_failed_runs: bool_field(object.get("saveFailedRuns"), default.save_failed_runs),
            failed_output_path: string_field(object.get("failedOutputPath"), &default.failed_output_path),
            failed_run_limit: non_negative_usize(object.get("failedRunLimit"), default.failed_run_limit),
            clip_filename_template: clip_filename_template(object.get("clipFilenameTemplate")),
            pre_run_padding_secs: non_negative_f64(object.get("preRunPaddingSecs"), default.pre_run_padding_secs),
            post_run_padding_secs: non_negative_f64(object.get("postRunPaddingSecs"), default.post_run_padding_secs),
            discord_notifications_enabled: bool_field(
                object.get("discordNotificationsEnabled"),
                default.discord_notifications_enabled,
            ),
            discord_webhook_url: string_field(object.get("discordWebhookUrl"), &default.discord_webhook_url),
            streaming_started_message_template: message_template(
                object.get("streamingStartedMessageTemplate"),
                DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE,
            ),
            streaming_stopped_message_template: message_template(
                object.get("streamingStoppedMessageTemplate"),
                DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE,
            ),
        }
    }

    pub fn recording_options(&self) -> RecordingOptions {
        RecordingOptions {
            completed_output_path: self.completed_output_path.trim().to_owned(),
            save_failed_runs: self.save_failed_runs,
            failed_output_path: self.failed_output_path.trim().to_owned(),
            failed_run_limit: self.failed_run_limit,
            clip_filename_template: self.clip_filename_template.trim().to_owned(),
            pre_run_padding_secs: self.pre_run_padding_secs,
            post_run_padding_secs: self.post_run_padding_secs,
        }
    }

    pub fn notification_options(&self) -> NotificationOptions {
        NotificationOptions {
            enabled: self.discord_notifications_enabled,
            discord_webhook_url: self.discord_webhook_url.trim().to_owned(),
            streaming_started_message_template: message_template_str(
                Some(&self.streaming_started_message_template),
                DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE,
            ),
            streaming_stopped_message_template: message_template_str(
                Some(&self.streaming_stopped_message_template),
                DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE,
            ),
        }
    }
}

/// Discord notification behaviour supplied by the frontend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationOptions {
    pub enabled: bool,
    pub discord_webhook_url: String,
    pub streaming_started_message_template: String,
    pub streaming_stopped_message_template: String,
}

/// In-memory settings plus the path where they are persisted. The mutex is held
/// only for short clones/replacements; disk IO happens outside the lock.
pub struct SettingsStore {
    path: PathBuf,
    settings: Mutex<AppSettings>,
}

impl SettingsStore {
    pub fn load_default() -> Self {
        Self::load_from_path(default_settings_path())
    }

    pub fn load_from_path(path: PathBuf) -> Self {
        let settings = if path.exists() {
            match read_settings(&path) {
                Ok(settings) => {
                    tracing::info!(path = %path.display(), "loaded settings");
                    settings
                }
                Err(err) => {
                    tracing::warn!(path = %path.display(), "using default settings: {err:#}");
                    AppSettings::default()
                }
            }
        } else {
            tracing::info!(path = %path.display(), "settings file not found; using defaults");
            AppSettings::default()
        };

        SettingsStore { path, settings: Mutex::new(settings) }
    }

    pub fn get(&self) -> AppSettings {
        self.settings.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn get_recording_options(&self) -> RecordingOptions {
        self.get().recording_options()
    }

    pub fn get_notification_options(&self) -> NotificationOptions {
        self.get().notification_options()
    }

    pub fn set_from_json_value(&self, value: Value) -> anyhow::Result<AppSettings> {
        let settings = AppSettings::from_json_value(value);
        write_settings(&self.path, &settings)?;

        let mut guard = self.settings.lock().unwrap_or_else(|p| p.into_inner());
        *guard = settings.clone();
        tracing::info!(path = %self.path.display(), "saved settings");

        Ok(settings)
    }
}

fn read_settings(path: &Path) -> anyhow::Result<AppSettings> {
    let bytes = std::fs::read(path).with_context(|| format!("reading settings file {}", path.display()))?;
    let value: Value =
        serde_json::from_slice(&bytes).with_context(|| format!("parsing settings file {}", path.display()))?;
    Ok(AppSettings::from_json_value(value))
}

fn write_settings(path: &Path, settings: &AppSettings) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating settings directory {}", parent.display()))?;
    }

    let bytes = serde_json::to_vec_pretty(settings).context("serializing settings")?;
    std::fs::write(path, bytes).with_context(|| format!("writing settings file {}", path.display()))
}

fn developer_lang(value: Option<&Value>, fallback: &str) -> String {
    match value.and_then(Value::as_str) {
        Some(lang @ ("en" | "jp")) => lang.to_owned(),
        _ => fallback.to_owned(),
    }
}

fn string_field(value: Option<&Value>, fallback: &str) -> String {
    value.and_then(Value::as_str).unwrap_or(fallback).to_owned()
}

fn bool_field(value: Option<&Value>, fallback: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(fallback)
}

fn clip_filename_template(value: Option<&Value>) -> String {
    let value = value.and_then(Value::as_str).unwrap_or(DEFAULT_CLIP_FILENAME_TEMPLATE);
    if value.is_empty() || value == LEGACY_CLIP_FILENAME_TEMPLATE {
        DEFAULT_CLIP_FILENAME_TEMPLATE.to_owned()
    } else {
        value.to_owned()
    }
}

fn message_template(value: Option<&Value>, fallback: &str) -> String {
    message_template_str(value.and_then(Value::as_str), fallback)
}

fn message_template_str(value: Option<&str>, fallback: &str) -> String {
    let value = value.unwrap_or(fallback);
    if value.trim().is_empty() { fallback.to_owned() } else { value.to_owned() }
}

fn non_negative_usize(value: Option<&Value>, fallback: usize) -> usize {
    number_value(value).filter(|n| n.is_finite()).map(|n| n.max(0.0).trunc() as usize).unwrap_or(fallback)
}

fn non_negative_f64(value: Option<&Value>, fallback: f64) -> f64 {
    number_value(value).filter(|n| n.is_finite()).map(|n| n.max(0.0)).unwrap_or(fallback)
}

fn number_value(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

pub fn default_settings_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("The Golden Eye")
                .join(SETTINGS_FILE_NAME);
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("The Golden Eye").join(SETTINGS_FILE_NAME);
        }
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            return PathBuf::from(profile)
                .join("AppData")
                .join("Roaming")
                .join("The Golden Eye")
                .join(SETTINGS_FILE_NAME);
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(config_home).join("the-golden-eye").join(SETTINGS_FILE_NAME);
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".config").join("the-golden-eye").join(SETTINGS_FILE_NAME);
        }
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(SETTINGS_FILE_NAME)
}

#[cfg(test)]
mod tests {
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
                let path =
                    std::env::temp_dir().join(format!("ge-settings-{label}-{}-{nanos}-{id}", std::process::id()));
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
        assert!(!AppSettings::default().stop_replay_buffer_when_monitor_stopped);
        assert_eq!(AppSettings::from_json_value(json!({})).pre_run_padding_secs, DEFAULT_PRE_RUN_PADDING_SECS);
        assert!(!AppSettings::from_json_value(json!({})).stop_replay_buffer_when_monitor_stopped);
    }

    #[test]
    fn json_value_is_normalized_field_by_field() {
        let settings = AppSettings::from_json_value(json!({
            "developerLang": "jp",
            "openGoldenEyeOnLaunch": false,
            "stopReplayBufferWhenMonitorStopped": true,
            "completedOutputPath": "/tmp/completed",
            "saveFailedRuns": false,
            "failedOutputPath": "/tmp/failed",
            "failedRunLimit": "7.9",
            "clipFilenameTemplate": LEGACY_CLIP_FILENAME_TEMPLATE,
            "preRunPaddingSecs": -3,
            "postRunPaddingSecs": "2.5",
            "discordNotificationsEnabled": false,
            "discordWebhookUrl": " https://discord.example/webhook ",
            "streamingStartedMessageTemplate": "",
            "streamingStoppedMessageTemplate": "Stopped {broadcast_url}"
        }));

        assert_eq!(settings.developer_lang, "jp");
        assert!(!settings.open_golden_eye_on_launch);
        assert!(settings.stop_replay_buffer_when_monitor_stopped);
        assert_eq!(settings.completed_output_path, "/tmp/completed");
        assert!(!settings.save_failed_runs);
        assert_eq!(settings.failed_output_path, "/tmp/failed");
        assert_eq!(settings.failed_run_limit, 7);
        assert_eq!(settings.clip_filename_template, DEFAULT_CLIP_FILENAME_TEMPLATE);
        assert_eq!(settings.pre_run_padding_secs, 0.0);
        assert_eq!(settings.post_run_padding_secs, 2.5);
        assert!(!settings.discord_notifications_enabled);
        assert_eq!(settings.discord_webhook_url, " https://discord.example/webhook ");
        assert_eq!(settings.streaming_started_message_template, DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE);
        assert_eq!(settings.streaming_stopped_message_template, "Stopped {broadcast_url}");

        let notification_options = settings.notification_options();
        assert!(!notification_options.enabled);
        assert_eq!(notification_options.discord_webhook_url, "https://discord.example/webhook");
    }

    #[test]
    fn store_persists_and_loads_settings_json() {
        let dir = TestDir::new("persist");
        let path = dir.join("nested/settings.json");
        let store = SettingsStore::load_from_path(path.clone());

        let saved = store
            .set_from_json_value(json!({
                "developerLang": "jp",
                "openGoldenEyeOnLaunch": false,
                "stopReplayBufferWhenMonitorStopped": true,
                "completedOutputPath": "/runs",
                "saveFailedRuns": true,
                "failedOutputPath": "/fails",
                "failedRunLimit": 3,
                "clipFilenameTemplate": "{level}",
                "preRunPaddingSecs": 1.25,
                "postRunPaddingSecs": 4,
                "discordNotificationsEnabled": false,
                "discordWebhookUrl": "https://discord.example/webhook",
                "streamingStartedMessageTemplate": "Started {broadcast_url}",
                "streamingStoppedMessageTemplate": "Stopped {broadcast_url}"
            }))
            .unwrap();

        assert_eq!(saved.completed_output_path, "/runs");
        assert!(!saved.open_golden_eye_on_launch);
        assert!(saved.stop_replay_buffer_when_monitor_stopped);
        assert!(path.exists());

        let reloaded = SettingsStore::load_from_path(path).get();
        assert_eq!(reloaded, saved);
    }
}
