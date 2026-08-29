use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::{Deserialize, Serialize};

use crate::ffi::queue_ui_task;

const PICKER_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderPickRequest {
    title: Option<String>,
    current_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderPickResponse {
    cancelled: bool,
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderValidateRequest {
    path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderValidation {
    expanded_path: String,
    empty: bool,
    exists: bool,
    is_directory: bool,
    writable: bool,
    will_create: bool,
    error: Option<String>,
}

struct FolderPickTask {
    title: String,
    start_dir: Option<PathBuf>,
    sender: mpsc::Sender<Option<PathBuf>>,
}

#[axum::debug_handler]
pub async fn handle_pick(Json(req): Json<FolderPickRequest>) -> Result<impl IntoResponse> {
    let title = req.title.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "Choose folder".to_owned());
    let start_dir = req.current_path.as_deref().and_then(initial_directory).or_else(default_videos_directory);

    let selected = tokio::task::spawn_blocking(move || pick_folder_on_ui_thread(title, start_dir))
        .await
        .map_err(|err| {
            tracing::error!("folder picker task failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "folder picker failed").into_response()
        })?
        .map_err(|err| {
            tracing::error!("folder picker failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "folder picker failed").into_response()
        })?;

    Ok((
        StatusCode::OK,
        Json(FolderPickResponse {
            cancelled: selected.is_none(),
            path: selected.map(|path| path.to_string_lossy().into_owned()),
        }),
    ))
}

#[axum::debug_handler]
pub async fn handle_validate(Json(req): Json<FolderValidateRequest>) -> Result<impl IntoResponse> {
    Ok((StatusCode::OK, Json(validate_folder_path(&req.path))))
}

fn pick_folder_on_ui_thread(title: String, start_dir: Option<PathBuf>) -> anyhow::Result<Option<PathBuf>> {
    let (sender, receiver) = mpsc::channel();
    let task = Box::new(FolderPickTask { title, start_dir, sender });
    let param = Box::into_raw(task).cast();

    queue_ui_task(pick_folder_task, param);

    receiver.recv_timeout(PICKER_TIMEOUT).context("waiting for folder picker")
}

unsafe extern "C" fn pick_folder_task(param: *mut std::ffi::c_void) {
    let task = unsafe { Box::from_raw(param.cast::<FolderPickTask>()) };
    let FolderPickTask { title, start_dir, sender } = *task;

    let mut dialog = rfd::FileDialog::new().set_title(title).set_can_create_directories(true);
    if let Some(start_dir) = start_dir {
        dialog = dialog.set_directory(start_dir);
    }
    let _ = sender.send(dialog.pick_folder());
}

fn validate_folder_path(raw: &str) -> FolderValidation {
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

fn initial_directory(raw: &str) -> Option<PathBuf> {
    let path = resolve_path(raw.trim());
    nearest_existing_directory(&path)
}

fn default_videos_directory() -> Option<PathBuf> {
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
