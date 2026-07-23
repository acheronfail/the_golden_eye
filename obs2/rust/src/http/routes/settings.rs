use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde_json::Value;

use crate::http::AppState;
use crate::settings::SettingsStatus;

/// Replaces the current settings and writes them to the platform config file.
/// The body is parsed field-by-field so future/missing/mistyped fields fall back
/// to safe defaults instead of poisoning the settings file.
#[axum::debug_handler]
pub async fn handle_put(State(state): State<AppState>, Json(value): Json<Value>) -> Result<impl IntoResponse> {
    match state.settings.set_from_json_value_with_runtime_defaults(value) {
        Ok(settings) => {
            state.snapshot.set_settings_status(state.settings.status());
            if let Err(err) = state.run_catalog.cleanup_recent(settings.recent_run_limit) {
                tracing::warn!("failed to apply recent-run limit after settings update: {err:#}");
            }
            let _ = state.event_tx.send(crate::http::AppEvent::RunCatalogChanged { run_id: None, save_id: None });
            Ok((StatusCode::OK, Json(settings)))
        }
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
        Ok(settings) => {
            state.snapshot.set_settings_status(state.settings.status());
            if let Err(err) = state.run_catalog.cleanup_recent(settings.recent_run_limit) {
                tracing::warn!("failed to apply recent-run limit after settings reset: {err:#}");
            }
            let _ = state.event_tx.send(crate::http::AppEvent::RunCatalogChanged { run_id: None, save_id: None });
            Ok((StatusCode::OK, Json(settings)))
        }
        Err(err) => {
            tracing::error!("failed to reset settings: {err:#}");
            Err((StatusCode::INTERNAL_SERVER_ERROR, "failed to reset settings").into())
        }
    }
}
