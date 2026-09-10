use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::Deserialize;

use crate::http::{AppEvent, AppState, MonitorStoppedReason};
use crate::monitor::{StartError, start_monitor, stop_monitor};

mod frame_dump;
pub use frame_dump::{FrameDumpHandle, handle_frame_dump};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartParams {
    /// Name of the OBS source to monitor, as reported by `/api/v1/sources`.
    source_name: String,
}

#[axum::debug_handler]
pub async fn handle_start(State(state): State<AppState>, Json(params): Json<StartParams>) -> Result<impl IntoResponse> {
    let effective_settings = state.settings.get_effective();
    let catalog_state = state.clone();
    tokio::task::spawn_blocking(move || {
        super::runs::seed_catalog_if_needed(&catalog_state, &effective_settings);
    })
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "run catalog task failed"))?;
    start_monitor(&state, params.source_name).map_err(start_error_response)?;
    Ok(StatusCode::OK)
}

fn start_error_response(error: StartError) -> (StatusCode, &'static str) {
    match error {
        StartError::InvalidSourceName => (StatusCode::BAD_REQUEST, "source name contains a null byte"),
        StartError::AlreadyRunning => (StatusCode::CONFLICT, "a monitor is already running"),
        StartError::ReplayBufferUnavailable => (StatusCode::PRECONDITION_FAILED, "replay buffer is unavailable"),
        StartError::MatcherUnavailable => (StatusCode::INTERNAL_SERVER_ERROR, "failed to init matcher"),
        StartError::CaptureUnavailable => (StatusCode::INTERNAL_SERVER_ERROR, "failed to create capture context"),
        StartError::WorkerUnavailable => (StatusCode::INTERNAL_SERVER_ERROR, "failed to spawn monitor thread"),
    }
}

#[axum::debug_handler]
pub async fn handle_stop(State(state): State<AppState>) -> Result<impl IntoResponse> {
    if !stop_monitor(&state, "userStopped").await {
        return Err((StatusCode::CONFLICT, "no monitor is running").into());
    }
    let _ = state.event_tx.send(AppEvent::MonitorStopped { reason: MonitorStoppedReason::UserStopped });
    Ok(StatusCode::OK)
}
