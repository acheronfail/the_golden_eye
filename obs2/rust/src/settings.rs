//! Persisted application settings.
//!
//! These are user-editable options shared between the SPA and Rust runtime. The
//! JSON file is intentionally owned by Rust so OBS-triggered workflows can
//! read the same configuration even when no browser tab is open.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;
use serde::Serialize;
use serde_json::Value;

use crate::recording::{
    DEFAULT_CLIP_FILENAME_TEMPLATE,
    DEFAULT_MINIMUM_FAILED_RUN_LENGTH_SECS,
    DEFAULT_POST_RUN_PADDING_SECS,
    DEFAULT_PRE_RUN_PADDING_SECS,
    RecordingOptions,
};
use crate::stream_notifier::{DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE, DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE};

const SETTINGS_FILE_NAME: &str = "settings.json";
const LEGACY_CLIP_FILENAME_TEMPLATE: &str = "{replay} - clip - {level}{time_suffix}{failed_suffix}";
pub const DEFAULT_UPDATE_CHECK_INTERVAL: UpdateCheckInterval = UpdateCheckInterval::Weekly;
pub const DEFAULT_RUN_OUTPUT_DIR_NAME: &str = "Goldeneye";
pub const DEFAULT_FAILED_OUTPUT_DIR_NAME: &str = "failed";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateCheckInterval {
    Monthly,
    Weekly,
    Daily,
    Never,
}

impl UpdateCheckInterval {
    pub fn from_json_value(value: Option<&Value>) -> Self {
        match value.and_then(Value::as_str) {
            Some("monthly") => UpdateCheckInterval::Monthly,
            Some("daily") => UpdateCheckInterval::Daily,
            Some("never") => UpdateCheckInterval::Never,
            Some("weekly") => UpdateCheckInterval::Weekly,
            _ => DEFAULT_UPDATE_CHECK_INTERVAL,
        }
    }

    pub fn interval_secs(self) -> Option<u64> {
        match self {
            UpdateCheckInterval::Daily => Some(24 * 60 * 60),
            UpdateCheckInterval::Weekly => Some(7 * 24 * 60 * 60),
            UpdateCheckInterval::Monthly => Some(30 * 24 * 60 * 60),
            UpdateCheckInterval::Never => None,
        }
    }
}

/// User settings stored in the plugin-owned JSON file and mirrored by the SPA's
/// bindable settings object.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub stop_replay_buffer_when_monitor_stopped: bool,
    pub show_monitor_fps: bool,
    pub show_developer_settings: bool,
    pub welcome_modal_shown: bool,
    pub completed_output_path: String,
    pub save_failed_runs: bool,
    pub failed_output_path: String,
    pub failed_run_limit: usize,
    pub minimum_failed_run_length_secs: f64,
    pub clip_filename_template: String,
    pub pre_run_padding_secs: f64,
    pub post_run_padding_secs: f64,
    pub discord_notifications_enabled: bool,
    pub discord_webhook_url: String,
    pub streaming_started_message_template: String,
    pub streaming_stopped_message_template: String,
    pub update_check_interval: UpdateCheckInterval,
    pub last_update_check_time: Option<u64>,
    pub auto_update_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            stop_replay_buffer_when_monitor_stopped: false,
            show_monitor_fps: false,
            show_developer_settings: false,
            welcome_modal_shown: false,
            completed_output_path: String::new(),
            save_failed_runs: true,
            failed_output_path: String::new(),
            failed_run_limit: 0,
            minimum_failed_run_length_secs: DEFAULT_MINIMUM_FAILED_RUN_LENGTH_SECS,
            clip_filename_template: DEFAULT_CLIP_FILENAME_TEMPLATE.to_owned(),
            pre_run_padding_secs: DEFAULT_PRE_RUN_PADDING_SECS,
            post_run_padding_secs: DEFAULT_POST_RUN_PADDING_SECS,
            discord_notifications_enabled: true,
            discord_webhook_url: String::new(),
            streaming_started_message_template: DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE.to_owned(),
            streaming_stopped_message_template: DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE.to_owned(),
            update_check_interval: DEFAULT_UPDATE_CHECK_INTERVAL,
            last_update_check_time: None,
            auto_update_enabled: false,
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
            stop_replay_buffer_when_monitor_stopped: bool_field(
                object.get("stopReplayBufferWhenMonitorStopped"),
                default.stop_replay_buffer_when_monitor_stopped,
            ),
            show_monitor_fps: bool_field(object.get("showMonitorFps"), default.show_monitor_fps),
            show_developer_settings: bool_field(object.get("showDeveloperSettings"), default.show_developer_settings),
            welcome_modal_shown: bool_field(object.get("welcomeModalShown"), default.welcome_modal_shown),
            completed_output_path: string_field(object.get("completedOutputPath"), &default.completed_output_path),
            save_failed_runs: bool_field(object.get("saveFailedRuns"), default.save_failed_runs),
            failed_output_path: string_field(object.get("failedOutputPath"), &default.failed_output_path),
            failed_run_limit: non_negative_usize(object.get("failedRunLimit"), default.failed_run_limit),
            minimum_failed_run_length_secs: non_negative_f64(
                object.get("minimumFailedRunLengthSecs"),
                default.minimum_failed_run_length_secs,
            ),
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
            update_check_interval: UpdateCheckInterval::from_json_value(object.get("updateCheckInterval")),
            last_update_check_time: non_negative_u64_option(object.get("lastUpdateCheckTime")),
            auto_update_enabled: bool_field(object.get("autoUpdateEnabled"), default.auto_update_enabled),
        }
    }

    pub fn recording_options(&self) -> RecordingOptions {
        RecordingOptions {
            completed_output_path: self.completed_output_path.trim().to_owned(),
            save_failed_runs: self.save_failed_runs,
            failed_output_path: self.failed_output_path.trim().to_owned(),
            failed_run_limit: self.failed_run_limit,
            minimum_failed_run_length_secs: self.minimum_failed_run_length_secs,
            clip_filename_template: self.clip_filename_template.trim().to_owned(),
            pre_run_padding_secs: self.pre_run_padding_secs,
            post_run_padding_secs: self.post_run_padding_secs,
        }
    }

    pub fn with_default_output_paths(mut self, replay_output_dir: Option<&Path>) -> Self {
        if self.completed_output_path.trim().is_empty()
            && let Some(replay_output_dir) = replay_output_dir
        {
            self.completed_output_path =
                default_completed_output_path(replay_output_dir).to_string_lossy().into_owned();
        }

        if self.failed_output_path.trim().is_empty()
            && let Some(path) = default_failed_output_path(&self.completed_output_path)
        {
            self.failed_output_path = path;
        }

        self
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
    state: Mutex<SettingsState>,
}

#[derive(Debug, Clone)]
struct SettingsState {
    settings: AppSettings,
    file_error: Option<String>,
    file_bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsStatus {
    pub settings: AppSettings,
    pub config_path: String,
    pub file_error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SettingsReload {
    Unchanged,
    Reloaded(AppSettings),
    Invalid(String),
}

impl SettingsStore {
    pub fn load_default() -> Self {
        Self::load_from_path(default_settings_path())
    }

    pub fn load_from_path(path: PathBuf) -> Self {
        let (settings, file_error, file_bytes) = match read_settings_file(&path) {
            Ok(Some((settings, bytes))) => {
                tracing::info!(path = %path.display(), "loaded settings");
                (settings, None, Some(bytes))
            }
            Ok(None) => {
                tracing::info!(path = %path.display(), "settings file not found; using defaults");
                (AppSettings::default(), None, None)
            }
            Err(err) => {
                tracing::warn!(path = %path.display(), "using default settings: {err:#}");
                (AppSettings::default(), Some(format!("{err:#}")), read_settings_bytes(&path).ok().flatten())
            }
        };

        SettingsStore { path, state: Mutex::new(SettingsState { settings, file_error, file_bytes }) }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn status(&self) -> SettingsStatus {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        SettingsStatus {
            settings: apply_runtime_output_path_defaults(state.settings.clone()),
            config_path: self.path.to_string_lossy().into_owned(),
            file_error: state.file_error.clone(),
        }
    }

    pub fn reload_from_disk_if_changed(&self) -> SettingsReload {
        let disk_bytes = match read_settings_bytes(&self.path) {
            Ok(bytes) => bytes,
            Err(err) => {
                let message = format!("{err:#}");
                let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                if state.file_error.as_deref() == Some(&message) {
                    return SettingsReload::Unchanged;
                }
                state.file_error = Some(message.clone());
                return SettingsReload::Invalid(message);
            }
        };

        {
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if state.file_bytes == disk_bytes {
                return SettingsReload::Unchanged;
            }
        }

        match parse_settings_bytes(&self.path, disk_bytes.as_deref()) {
            Ok(settings) => {
                let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                state.settings = settings.clone();
                state.file_error = None;
                state.file_bytes = disk_bytes;
                tracing::info!(path = %self.path.display(), "reloaded settings");
                SettingsReload::Reloaded(apply_runtime_output_path_defaults(settings))
            }
            Err(err) => {
                let message = format!("{err:#}");
                let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                state.file_error = Some(message.clone());
                state.file_bytes = disk_bytes;
                tracing::warn!(path = %self.path.display(), "settings file is invalid: {err:#}");
                SettingsReload::Invalid(message)
            }
        }
    }

    pub fn ensure_file_exists(&self) -> anyhow::Result<()> {
        if self.path.exists() {
            return Ok(());
        }

        let settings = self.get();
        let bytes = write_settings(&self.path, &settings)?;
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.file_bytes = Some(bytes);
        state.file_error = None;
        Ok(())
    }

    pub fn reset_to_defaults(&self) -> anyhow::Result<AppSettings> {
        self.replace(apply_runtime_output_path_defaults(AppSettings::default()))
    }

    pub fn get(&self) -> AppSettings {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).settings.clone()
    }

    pub fn get_effective(&self) -> AppSettings {
        apply_runtime_output_path_defaults(self.get())
    }

    pub fn get_recording_options(&self) -> RecordingOptions {
        self.get_effective().recording_options()
    }

    pub fn get_notification_options(&self) -> NotificationOptions {
        self.get().notification_options()
    }

    pub fn set_last_update_check_time(&self, seconds: u64) -> anyhow::Result<AppSettings> {
        let mut settings = self.get();
        settings.last_update_check_time = Some(seconds);
        self.replace(settings)
    }

    pub fn set_from_json_value_with_runtime_defaults(&self, value: Value) -> anyhow::Result<AppSettings> {
        if let Some(error) = self.state.lock().unwrap_or_else(|p| p.into_inner()).file_error.clone() {
            anyhow::bail!("settings file is invalid; fix it or reset to defaults before saving: {error}");
        }

        let mut settings = apply_runtime_output_path_defaults(AppSettings::from_json_value(value));
        settings.last_update_check_time = self.get().last_update_check_time;
        self.replace(settings)
    }

    fn replace(&self, settings: AppSettings) -> anyhow::Result<AppSettings> {
        let bytes = write_settings(&self.path, &settings)?;

        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.settings = settings.clone();
        state.file_error = None;
        state.file_bytes = Some(bytes);
        tracing::info!(path = %self.path.display(), "saved settings");

        Ok(settings)
    }
}

fn read_settings_file(path: &Path) -> anyhow::Result<Option<(AppSettings, Vec<u8>)>> {
    match read_settings_bytes(path)? {
        Some(bytes) => {
            let settings = parse_settings_bytes(path, Some(&bytes))?;
            Ok(Some((settings, bytes)))
        }
        None => Ok(None),
    }
}

fn read_settings_bytes(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("reading settings file {}", path.display())),
    }
}

fn parse_settings_bytes(path: &Path, bytes: Option<&[u8]>) -> anyhow::Result<AppSettings> {
    match bytes {
        Some(bytes) => {
            let value: Value =
                serde_json::from_slice(bytes).with_context(|| format!("parsing settings file {}", path.display()))?;
            Ok(AppSettings::from_json_value(value))
        }
        None => Ok(AppSettings::default()),
    }
}

fn apply_runtime_output_path_defaults(settings: AppSettings) -> AppSettings {
    let replay_output_dir = crate::recording::replay_buffer_output_directory();
    settings.with_default_output_paths(replay_output_dir.as_deref())
}

pub fn default_completed_output_path(replay_output_dir: &Path) -> PathBuf {
    replay_output_dir.join(DEFAULT_RUN_OUTPUT_DIR_NAME)
}

pub fn default_failed_output_path(completed_output_path: &str) -> Option<String> {
    let completed_output_path = completed_output_path.trim();
    if completed_output_path.is_empty() {
        None
    } else {
        Some(Path::new(completed_output_path).join(DEFAULT_FAILED_OUTPUT_DIR_NAME).to_string_lossy().into_owned())
    }
}

fn write_settings(path: &Path, settings: &AppSettings) -> anyhow::Result<Vec<u8>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating settings directory {}", parent.display()))?;
    }

    let bytes = serde_json::to_vec_pretty(settings).context("serializing settings")?;
    std::fs::write(path, &bytes).with_context(|| format!("writing settings file {}", path.display()))?;
    Ok(bytes)
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

fn non_negative_u64_option(value: Option<&Value>) -> Option<u64> {
    number_value(value).filter(|n| n.is_finite()).map(|n| n.max(0.0).trunc() as u64)
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
        assert_eq!(AppSettings::default().minimum_failed_run_length_secs, DEFAULT_MINIMUM_FAILED_RUN_LENGTH_SECS);
        assert!(!AppSettings::default().stop_replay_buffer_when_monitor_stopped);
        assert!(!AppSettings::default().show_monitor_fps);
        assert!(!AppSettings::default().show_developer_settings);
        assert!(!AppSettings::default().welcome_modal_shown);
        assert_eq!(AppSettings::default().update_check_interval, UpdateCheckInterval::Weekly);
        assert_eq!(AppSettings::default().last_update_check_time, None);
        assert_eq!(AppSettings::from_json_value(json!({})).pre_run_padding_secs, DEFAULT_PRE_RUN_PADDING_SECS);
        assert_eq!(
            AppSettings::from_json_value(json!({})).minimum_failed_run_length_secs,
            DEFAULT_MINIMUM_FAILED_RUN_LENGTH_SECS
        );
        assert!(!AppSettings::from_json_value(json!({})).stop_replay_buffer_when_monitor_stopped);
        assert!(!AppSettings::from_json_value(json!({})).show_monitor_fps);
        assert!(!AppSettings::from_json_value(json!({})).show_developer_settings);
        assert!(!AppSettings::from_json_value(json!({})).welcome_modal_shown);
        assert_eq!(AppSettings::from_json_value(json!({})).update_check_interval, UpdateCheckInterval::Weekly);
        assert_eq!(AppSettings::from_json_value(json!({})).last_update_check_time, None);
    }

    #[test]
    fn json_value_is_normalized_field_by_field() {
        let settings = AppSettings::from_json_value(json!({
            "stopReplayBufferWhenMonitorStopped": true,
            "showMonitorFps": true,
            "showDeveloperSettings": true,
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
            "lastUpdateCheckTime": "1234.9"
        }));

        assert!(settings.stop_replay_buffer_when_monitor_stopped);
        assert!(settings.show_monitor_fps);
        assert!(settings.show_developer_settings);
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

        assert_eq!(settings.completed_output_path, "/tmp/obs-replays/Goldeneye");
        assert_eq!(settings.failed_output_path, "/tmp/obs-replays/Goldeneye/failed");

        let custom_completed =
            AppSettings::from_json_value(json!({ "completedOutputPath": "/runs" })).with_default_output_paths(None);
        assert_eq!(custom_completed.completed_output_path, "/runs");
        assert_eq!(custom_completed.failed_output_path, "/runs/failed");
    }

    #[test]
    fn store_persists_and_loads_settings_json() {
        let dir = TestDir::new("persist");
        let path = dir.join("nested/settings.json");
        let store = SettingsStore::load_from_path(path.clone());

        let saved = store
            .replace(AppSettings::from_json_value(json!({
                "stopReplayBufferWhenMonitorStopped": true,
                "showMonitorFps": true,
                "showDeveloperSettings": true,
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
        assert!(saved.show_monitor_fps);
        assert!(saved.show_developer_settings);
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
}
