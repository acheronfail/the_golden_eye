use std::path::PathBuf;
use std::process::Command;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::Deserialize;

use super::runs;
use crate::http::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", tag = "target")]
pub enum FileRevealRequest {
    Run { path: String },
    RunFolder { kind: runs::RunDirectoryKind },
    SettingsConfig,
}

#[derive(Debug, Clone, Copy)]
enum RevealMode {
    Select,
    Open,
}

#[axum::debug_handler]
pub async fn handle_reveal(
    State(state): State<AppState>,
    Json(req): Json<FileRevealRequest>,
) -> Result<impl IntoResponse> {
    let (path, mode) = match req {
        FileRevealRequest::Run { path } => {
            let settings = state.settings.get_effective();
            let path = runs::authorize_tagged_run_path(&settings, &path).map_err(runs::RunPathError::into_response)?;
            (path, RevealMode::Select)
        }
        FileRevealRequest::RunFolder { kind } => {
            let settings = state.settings.get_effective();
            let path =
                runs::configured_run_directory_for_kind(&settings, kind).map_err(runs::RunPathError::into_response)?;
            runs::ensure_configured_run_directory(&path).map_err(|err| {
                tracing::error!("failed to prepare run folder before reveal: {err:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "run folder reveal failed").into_response()
            })?;
            (path, RevealMode::Open)
        }
        FileRevealRequest::SettingsConfig => {
            let path = state.settings.path().to_path_buf();
            state.settings.ensure_file_exists().map_err(|err| {
                tracing::error!("failed to create settings file before reveal: {err:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "failed to create settings file").into_response()
            })?;
            (path, RevealMode::Select)
        }
    };

    tokio::task::spawn_blocking(move || reveal_in_file_browser(path, mode))
        .await
        .map_err(|err| {
            tracing::error!("file reveal task failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "file reveal failed").into_response()
        })?
        .map_err(|err| {
            tracing::error!("file reveal failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "file reveal failed").into_response()
        })?;

    Ok(StatusCode::NO_CONTENT)
}

fn reveal_in_file_browser(path: PathBuf, mode: RevealMode) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    let status = match mode {
        RevealMode::Select => Command::new("open").arg("-R").arg(&path).status(),
        RevealMode::Open => Command::new("open").arg(&path).status(),
    };

    #[cfg(target_os = "windows")]
    let status = match mode {
        RevealMode::Select => Command::new("explorer").arg(format!("/select,{}", path.display())).status(),
        RevealMode::Open => Command::new("explorer").arg(&path).status(),
    };

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let status = match mode {
        RevealMode::Select => {
            let target = path.parent().unwrap_or_else(|| std::path::Path::new("."));
            Command::new("xdg-open").arg(target).status()
        }
        RevealMode::Open => Command::new("xdg-open").arg(&path).status(),
    };

    let status = status?;
    if status.success() { Ok(()) } else { anyhow::bail!("file browser exited with status {status}") }
}
