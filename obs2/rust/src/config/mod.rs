mod browser_dock;
mod cv;
mod flatpak;
mod logging;
mod monitor;
mod paths;
mod shared;
mod updates;
mod youtube;

#[cfg(test)]
pub(crate) use browser_dock::DEFAULT_BROWSER_DOCK_URL;
pub(crate) use browser_dock::{browser_dock_disabled, browser_dock_url};
pub(crate) use cv::{cv_debug_enabled, cv_threads_overridden, cv_timing_enabled};
pub(crate) use flatpak::{flatpak_id, xdg_runtime_dir};
pub(crate) use logging::logging_filter;
pub(crate) use monitor::{MonitorTimingMode, default_monitor_slow_ms};
pub(crate) use paths::{current_dir, default_settings_path, home_dir, temp_dir};
#[cfg(test)]
pub(crate) use shared::env_value_enabled;
pub(crate) use updates::UpdateEnvConfig;
#[cfg(test)]
pub(crate) use updates::{LATEST_RELEASE_API_URL, RELEASES_API_URL};
#[cfg(test)]
pub(crate) use youtube::REDIRECT_URI;
pub(crate) use youtube::{YoutubeEndpoints, youtube_enabled};
#[cfg(feature = "test-hooks")]
pub(crate) use youtube::{test_oauth_state, token_file_override};
