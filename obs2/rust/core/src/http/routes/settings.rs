use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde_json::Value;

use crate::app::AppState;
use crate::settings::SettingsStatus;

/// Replaces the current settings and writes them to the platform config file.
/// Missing fields receive contract defaults; invalid field types are rejected
/// so a malformed manual edit is visible instead of silently changing values.
#[axum::debug_handler]
pub async fn handle_put(State(state): State<AppState>, Json(value): Json<Value>) -> Result<impl IntoResponse> {
    match state.save_settings(value) {
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
    match state.reset_settings() {
        Ok(settings) => Ok((StatusCode::OK, Json(settings))),
        Err(err) => {
            tracing::error!("failed to reset settings: {err:#}");
            Err((StatusCode::INTERNAL_SERVER_ERROR, "failed to reset settings").into())
        }
    }
}
