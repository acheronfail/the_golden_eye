use std::env;
use std::path::PathBuf;

use super::EnvVar;
use crate::settings::SETTINGS_FILE_NAME;

#[cfg(target_os = "windows")]
static APPDATA: EnvVar = EnvVar::new("APPDATA");
#[cfg(not(target_os = "windows"))]
static HOME: EnvVar = EnvVar::new("HOME");
#[cfg(target_os = "windows")]
static USERPROFILE: EnvVar = EnvVar::new("USERPROFILE");
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
static XDG_CONFIG_HOME: EnvVar = EnvVar::new("XDG_CONFIG_HOME");

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
        if let Some(appdata) = APPDATA.os() {
            return PathBuf::from(appdata).join("The Golden Eye").join(SETTINGS_FILE_NAME);
        }
        if let Some(profile) = home_dir() {
            return profile.join("AppData").join("Roaming").join("The Golden Eye").join(SETTINGS_FILE_NAME);
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Some(config_home) = XDG_CONFIG_HOME.os() {
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
        USERPROFILE.os().map(PathBuf::from)
    }

    #[cfg(not(target_os = "windows"))]
    {
        HOME.os().map(PathBuf::from)
    }
}
