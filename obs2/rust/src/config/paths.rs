use std::env;
use std::path::PathBuf;

use super::shared::env_os;
use crate::settings::SETTINGS_FILE_NAME;

#[cfg(target_os = "windows")]
const APPDATA: &str = "APPDATA";
const HOME: &str = "HOME";
#[cfg(target_os = "windows")]
const USERPROFILE: &str = "USERPROFILE";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const XDG_CONFIG_HOME: &str = "XDG_CONFIG_HOME";

/// Controls where the backend reads and writes the persistent settings file.
pub(crate) fn default_settings_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home_dir() {
            return home.join("Library").join("Application Support").join("The Golden Eye").join(SETTINGS_FILE_NAME);
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = env_os(APPDATA) {
            return PathBuf::from(appdata).join("The Golden Eye").join(SETTINGS_FILE_NAME);
        }
        if let Some(profile) = home_dir() {
            return profile.join("AppData").join("Roaming").join("The Golden Eye").join(SETTINGS_FILE_NAME);
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Some(config_home) = env_os(XDG_CONFIG_HOME) {
            return PathBuf::from(config_home).join("the-golden-eye").join(SETTINGS_FILE_NAME);
        }
        if let Some(home) = home_dir() {
            return home.join(".config").join("the-golden-eye").join(SETTINGS_FILE_NAME);
        }
    }

    current_dir().join(SETTINGS_FILE_NAME)
}

/// Controls how relative user-provided paths are resolved.
pub(crate) fn current_dir() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Controls where transient diagnostic files are created.
pub(crate) fn temp_dir() -> PathBuf {
    env::temp_dir()
}

/// Controls home-directory expansion and default user media paths.
pub(crate) fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        env_os(USERPROFILE).map(PathBuf::from)
    }

    #[cfg(not(target_os = "windows"))]
    {
        env_os(HOME).map(PathBuf::from)
    }
}
