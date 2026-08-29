//! Persisted application settings: user-editable options shared between the SPA and
//! Rust runtime. The JSON file is owned by Rust so OBS-triggered workflows can read
//! the same configuration even when no browser tab is open.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;
use serde::Serialize;
use serde_json::Value;
use settings_contract::{AppSettings as ContractSettings, MAX_RECENT_RUN_LIMIT};
#[cfg(test)]
use settings_contract::{
    DEFAULT_CLIP_FILENAME_TEMPLATE,
    DEFAULT_PRE_RUN_PADDING_SECS,
    DEFAULT_RECENT_RUN_LIMIT,
    DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE,
    MonitorDesign,
};
pub use settings_contract::{UpdateCheckInterval, YoutubeVisibility};

use crate::recording::RecordingOptions;

pub(crate) const SETTINGS_FILE_NAME: &str = "settings.json";
pub const DEFAULT_RUN_OUTPUT_DIR_NAME: &str = "GoldenEye";

/// Runtime wrapper around the generated settings contract. It keeps
/// OBS-dependent projections out of the lightweight contract crate.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(transparent)]
pub struct AppSettings(ContractSettings);

impl std::ops::Deref for AppSettings {
    type Target = ContractSettings;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for AppSettings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl AppSettings {
    pub fn from_json_value(value: Value) -> anyhow::Result<Self> {
        Ok(Self(ContractSettings::from_json_value(value)?))
    }

    pub fn recording_options(&self) -> RecordingOptions {
        RecordingOptions {
            completed_output_path: self.completed_output_path.trim().to_owned(),
            recent_run_limit: self.recent_run_limit.clamp(1, MAX_RECENT_RUN_LIMIT),
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

        self
    }

    pub fn notification_options(&self) -> NotificationOptions {
        NotificationOptions {
            enabled: self.discord_notifications_enabled,
            discord_webhook_url: self.discord_webhook_url.trim().to_owned(),
            streaming_started_message_template: self.streaming_started_message_template.clone(),
            streaming_stopped_message_template: self.streaming_stopped_message_template.clone(),
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

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsStatus {
    pub settings: AppSettings,
    pub defaults: AppSettings,
    pub config_path: String,
    pub plugin_version: String,
    pub file_error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SettingsReload {
    Unchanged,
    Reloaded(Box<AppSettings>),
    Invalid(String),
}

impl SettingsStore {
    pub fn load_default() -> Self {
        Self::load_from_path(crate::config::default_settings_path())
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
        self.status_with(apply_runtime_output_path_defaults)
    }

    /// Status snapshot that avoids OBS FFI. Used during plugin load before OBS
    /// frontend APIs are safe; replaced with `status()` after OBS post-load.
    pub fn status_without_runtime_defaults(&self) -> SettingsStatus {
        self.status_with(|settings| settings)
    }

    fn status_with(&self, apply_defaults: impl Fn(AppSettings) -> AppSettings) -> SettingsStatus {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        SettingsStatus {
            settings: apply_defaults(state.settings.clone()),
            defaults: apply_defaults(AppSettings::default()),
            config_path: self.path.to_string_lossy().into_owned(),
            plugin_version: crate::PLUGIN_VERSION.to_owned(),
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
                SettingsReload::Reloaded(Box::new(apply_runtime_output_path_defaults(settings)))
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
        self.update(|current| {
            let mut settings = current.clone();
            settings.last_update_check_time = Some(seconds);
            Ok(settings)
        })
    }

    pub fn set_last_known_update(&self, version: &str, release_url: &str) -> anyhow::Result<AppSettings> {
        self.update(|current| {
            let mut settings = current.clone();
            settings.last_known_update_version = Some(version.to_owned());
            settings.last_known_update_release_url = Some(release_url.to_owned());
            Ok(settings)
        })
    }

    pub fn set_from_json_value_with_runtime_defaults(&self, value: Value) -> anyhow::Result<AppSettings> {
        if let Some(error) = self.state.lock().unwrap_or_else(|p| p.into_inner()).file_error.clone() {
            anyhow::bail!("settings file is invalid; fix it or reset to defaults before saving: {error}");
        }

        self.update(|current| {
            let mut settings = apply_runtime_output_path_defaults(AppSettings::from_json_value(value.clone())?);
            settings.last_update_check_time = current.last_update_check_time;
            settings.last_known_update_version = current.last_known_update_version.clone();
            settings.last_known_update_release_url = current.last_known_update_release_url.clone();
            Ok(settings)
        })
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

    /// Builds a new settings value from the current one and persists it, retrying if
    /// another writer committed in between (a read-modify-write lost update, e.g. the
    /// update-check task racing a PUT). `build` may re-run, so it must be side-effect-free.
    fn update(&self, build: impl Fn(&AppSettings) -> anyhow::Result<AppSettings>) -> anyhow::Result<AppSettings> {
        loop {
            let before = self.get();
            let settings = build(&before)?;
            let bytes = write_settings(&self.path, &settings)?;

            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if state.settings != before {
                // Lost the race: someone else's write landed while we were
                // building/writing ours. Retry against their fresher value
                // instead of clobbering it.
                continue;
            }
            state.settings = settings.clone();
            state.file_error = None;
            state.file_bytes = Some(bytes);
            tracing::info!(path = %self.path.display(), "saved settings");
            return Ok(settings);
        }
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
            AppSettings::from_json_value(value)
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

fn write_settings(path: &Path, settings: &AppSettings) -> anyhow::Result<Vec<u8>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating settings directory {}", parent.display()))?;
    }

    let bytes = serde_json::to_vec_pretty(settings).context("serializing settings")?;
    std::fs::write(path, &bytes).with_context(|| format!("writing settings file {}", path.display()))?;
    Ok(bytes)
}

#[cfg(test)]
#[path = "settings_test.rs"]
mod settings_test;
