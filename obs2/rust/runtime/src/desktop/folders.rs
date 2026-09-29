use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use serde::Serialize;

const PICKER_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "FolderPickResult", rename_all = "camelCase")]
pub struct FolderPickResponse {
    pub(crate) cancelled: bool,
    pub(crate) path: Option<String>,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct FolderValidation {
    pub(crate) expanded_path: String,
    pub(crate) empty: bool,
    pub(crate) exists: bool,
    pub(crate) is_directory: bool,
    pub(crate) writable: bool,
    pub(crate) will_create: bool,
    pub(crate) error: Option<String>,
}

pub(crate) fn pick_folder_on_ui_thread(title: String, start_dir: Option<PathBuf>) -> anyhow::Result<Option<PathBuf>> {
    let (sender, receiver) = mpsc::channel();
    crate::obs::queue_ui_task(move || {
        let mut dialog = rfd::FileDialog::new().set_title(title).set_can_create_directories(true);
        if let Some(start_dir) = start_dir {
            dialog = dialog.set_directory(start_dir);
        }
        let _ = sender.send(dialog.pick_folder());
    });

    receiver.recv_timeout(PICKER_TIMEOUT).context("waiting for folder picker")
}

pub(crate) fn validate_folder_path(raw: &str) -> FolderValidation {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return FolderValidation {
            expanded_path: String::new(),
            empty: true,
            exists: false,
            is_directory: false,
            writable: true,
            will_create: false,
            error: None,
        };
    }

    let path = resolve_path(trimmed);
    let expanded_path = path.to_string_lossy().into_owned();

    match fs::metadata(&path) {
        Ok(metadata) if metadata.is_dir() => match probe_writable(&path) {
            Ok(()) => FolderValidation {
                expanded_path,
                empty: false,
                exists: true,
                is_directory: true,
                writable: true,
                will_create: false,
                error: None,
            },
            Err(err) => FolderValidation {
                expanded_path,
                empty: false,
                exists: true,
                is_directory: true,
                writable: false,
                will_create: false,
                error: Some(format!("Folder is not writable: {err}")),
            },
        },
        Ok(_) => FolderValidation {
            expanded_path,
            empty: false,
            exists: true,
            is_directory: false,
            writable: false,
            will_create: false,
            error: Some("Path exists but is not a folder.".to_owned()),
        },
        Err(err) if err.kind() == ErrorKind::NotFound => match nearest_existing_directory(&path) {
            Some(parent) => match probe_writable(&parent) {
                Ok(()) => FolderValidation {
                    expanded_path,
                    empty: false,
                    exists: false,
                    is_directory: false,
                    writable: true,
                    will_create: true,
                    error: None,
                },
                Err(err) => FolderValidation {
                    expanded_path,
                    empty: false,
                    exists: false,
                    is_directory: false,
                    writable: false,
                    will_create: false,
                    error: Some(format!("Parent folder is not writable: {err}")),
                },
            },
            None => FolderValidation {
                expanded_path,
                empty: false,
                exists: false,
                is_directory: false,
                writable: false,
                will_create: false,
                error: Some("No parent folder exists.".to_owned()),
            },
        },
        Err(err) => FolderValidation {
            expanded_path,
            empty: false,
            exists: false,
            is_directory: false,
            writable: false,
            will_create: false,
            error: Some(format!("Cannot read path: {err}")),
        },
    }
}

pub(crate) fn initial_directory(raw: &str) -> Option<PathBuf> {
    let path = resolve_path(raw.trim());
    nearest_existing_directory(&path)
}

pub(crate) fn default_videos_directory() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let candidate = crate::config::home_dir()?.join("Movies");

    #[cfg(not(target_os = "macos"))]
    let candidate = crate::config::home_dir()?.join("Videos");

    if candidate.is_dir() { Some(candidate) } else { crate::config::home_dir() }
}

fn nearest_existing_directory(path: &Path) -> Option<PathBuf> {
    let mut candidate = Some(path);
    while let Some(path) = candidate {
        if let Ok(metadata) = fs::metadata(path)
            && metadata.is_dir()
        {
            return Some(path.to_path_buf());
        }
        candidate = path.parent();
    }
    None
}

fn resolve_path(path: &str) -> PathBuf {
    let expanded = expand_home(path);
    if expanded.is_absolute() { expanded } else { crate::config::current_dir().join(expanded) }
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~"
        && let Some(home) = crate::config::home_dir()
    {
        return home;
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = crate::config::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

fn probe_writable(dir: &Path) -> anyhow::Result<()> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();

    for i in 0..16 {
        let candidate = dir.join(format!(".the-golden-eye-write-test-{}-{nanos}-{i}", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(_) => {
                let _ = fs::remove_file(&candidate);
                return Ok(());
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err).with_context(|| format!("writing {}", dir.display())),
        }
    }

    anyhow::bail!("could not create a unique write-test file in {}", dir.display())
}

#[cfg(test)]
#[path = "folders_test.rs"]
mod folders_test;
