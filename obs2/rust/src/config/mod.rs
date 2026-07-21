use std::env;
use std::ffi::OsString;
#[cfg(not(feature = "test-hooks"))]
use std::sync::OnceLock;

mod browser;
mod browser_dock;
mod cv;
mod flatpak;
mod logging;
pub mod matcher;
mod monitor;
mod paths;
mod updates;
mod youtube;

/// A named runtime environment variable with one-time detection logging.
pub(crate) struct EnvVar {
    key: &'static str,
    #[cfg(not(feature = "test-hooks"))]
    value: OnceLock<Option<OsString>>,
}

impl EnvVar {
    pub(crate) const fn new(key: &'static str) -> Self {
        Self {
            key,
            #[cfg(not(feature = "test-hooks"))]
            value: OnceLock::new(),
        }
    }

    pub(crate) const fn key(&self) -> &'static str {
        self.key
    }

    pub(crate) fn string(&self) -> Option<String> {
        self.read().and_then(|value| value.into_string().ok())
    }

    pub(crate) fn os(&self) -> Option<OsString> {
        self.read()
    }

    pub(crate) fn truthy(&self) -> bool {
        self.string().as_deref().is_some_and(Self::truthy_value)
    }

    pub(crate) fn is_set(&self) -> bool {
        self.read().is_some()
    }

    pub(crate) fn truthy_value(value: &str) -> bool {
        matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
    }

    #[cfg(not(feature = "test-hooks"))]
    fn read(&self) -> Option<OsString> {
        self.value.get_or_init(|| self.read_current()).clone()
    }

    #[cfg(feature = "test-hooks")]
    fn read(&self) -> Option<OsString> {
        self.read_current()
    }

    fn read_current(&self) -> Option<OsString> {
        let value = env::var_os(self.key);
        if value.is_some() {
            tracing::info!(env = self.key, "environment variable detected");
        }
        value
    }
}

pub(crate) use browser::browser_ws_log_enabled;
#[cfg(test)]
pub(crate) use browser_dock::DEFAULT_BROWSER_DOCK_URL;
pub(crate) use browser_dock::{browser_dock_disabled, browser_dock_url};
pub(crate) use cv::{cv_debug_enabled, cv_threads_overridden, cv_timing_enabled};
pub(crate) use flatpak::{flatpak_id, xdg_runtime_dir};
pub(crate) use logging::logging_filter;
pub(crate) use monitor::{MonitorTimingMode, default_monitor_slow_ms};
pub(crate) use paths::{current_dir, default_settings_path, home_dir, temp_dir};
pub(crate) use updates::UpdateEnvConfig;
#[cfg(test)]
pub(crate) use updates::{LATEST_RELEASE_API_URL, RELEASES_API_URL};
#[cfg(test)]
pub(crate) use youtube::REDIRECT_URI;
pub(crate) use youtube::{YoutubeEndpoints, client_secret, youtube_enabled};
#[cfg(feature = "test-hooks")]
pub(crate) use youtube::{force_keyring_failure, test_oauth_state, token_file_override};
