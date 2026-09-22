use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::Deserialize;

use super::runs::run_error_response;
use crate::app::AppState;
use crate::desktop::files::{RevealMode, reveal_in_file_browser};
use crate::run_library;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", tag = "target")]
pub enum FileRevealRequest {
    Run { path: String },
    RunFolder { kind: run_library::RunDirectoryKind },
    SettingsConfig,
}

#[axum::debug_handler]
pub async fn handle_reveal(
    State(state): State<AppState>,
    Json(req): Json<FileRevealRequest>,
) -> Result<impl IntoResponse> {
    let (path, mode) = match req {
        FileRevealRequest::Run { path } => {
            let settings = state.settings.get_effective();
            let path = run_library::authorize_tagged_run_path(&settings, &path).map_err(run_error_response)?;
            (path, RevealMode::Select)
        }
        FileRevealRequest::RunFolder { kind } => {
            let settings = state.settings.get_effective();
            let path = run_library::configured_run_directory_for_kind(&settings, kind).map_err(run_error_response)?;
            run_library::ensure_configured_run_directory(&path).map_err(|err| {
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
