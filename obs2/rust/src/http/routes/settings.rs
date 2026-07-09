use std::path::Path;
use std::process::Command;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde_json::Value;

use crate::http::AppState;
use crate::settings::{AppSettings, SettingsStatus};

/// Returns the current plugin-owned settings. The SPA hydrates its bindable
/// settings object from this on load.
#[axum::debug_handler]
pub async fn handle_get(State(state): State<AppState>) -> Json<AppSettings> {
    Json(state.settings.get_effective())
}

/// Replaces the current settings and writes them to the platform config file.
/// The body is parsed field-by-field so future/missing/mistyped fields fall back
/// to safe defaults instead of poisoning the settings file.
#[axum::debug_handler]
pub async fn handle_put(State(state): State<AppState>, Json(value): Json<Value>) -> Result<impl IntoResponse> {
    match state.settings.set_from_json_value_with_runtime_defaults(value) {
        Ok(settings) => Ok((StatusCode::OK, Json(settings))),
        Err(err) => {
            tracing::error!("failed to save settings: {err:#}");
            if state.settings.status().file_error.is_some() {
                Err((StatusCode::CONFLICT, "settings file is invalid; fix it or reset to defaults").into())
            } else {
                Err((StatusCode::INTERNAL_SERVER_ERROR, "failed to save settings").into())
            }
        }
    }
}

#[axum::debug_handler]
pub async fn handle_status(State(state): State<AppState>) -> Json<SettingsStatus> {
    Json(state.settings.status())
}

#[axum::debug_handler]
pub async fn handle_reset(State(state): State<AppState>) -> Result<impl IntoResponse> {
    match state.settings.reset_to_defaults() {
        Ok(settings) => Ok((StatusCode::OK, Json(settings))),
        Err(err) => {
            tracing::error!("failed to reset settings: {err:#}");
            Err((StatusCode::INTERNAL_SERVER_ERROR, "failed to reset settings").into())
        }
    }
}

#[axum::debug_handler]
pub async fn handle_reveal(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let path = state.settings.path().to_path_buf();
    state.settings.ensure_file_exists().map_err(|err| {
        tracing::error!("failed to create settings file before reveal: {err:#}");
        (StatusCode::INTERNAL_SERVER_ERROR, "failed to create settings file").into_response()
    })?;

    tokio::task::spawn_blocking(move || reveal_in_file_browser(&path))
        .await
        .map_err(|err| {
            tracing::error!("settings reveal task failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "settings reveal failed").into_response()
        })?
        .map_err(|err| {
            tracing::error!("settings reveal failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "settings reveal failed").into_response()
        })?;

    Ok(StatusCode::NO_CONTENT)
}

fn reveal_in_file_browser(path: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg("-R").arg(path).status();

    #[cfg(target_os = "windows")]
    let status = Command::new("explorer").arg(format!("/select,{}", path.display())).status();

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let status = match path.parent() {
        Some(parent) => Command::new("xdg-open").arg(parent).status(),
        None => Command::new("xdg-open").arg(path).status(),
    };

    let status = status?;
    if status.success() { Ok(()) } else { anyhow::bail!("file browser exited with status {status}") }
}
