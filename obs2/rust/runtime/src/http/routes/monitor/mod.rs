use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::Deserialize;

use crate::app::AppState;
use crate::run_monitoring::{StartError, StopReason};

mod frame_dump;
pub use frame_dump::handle_frame_dump;

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
        catalog_state.runs.seed_if_needed(&effective_settings);
    })
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "run catalog task failed"))?;
    tokio::task::spawn_blocking(move || state.monitor.start(params.source_name))
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "monitor startup task failed"))?
        .map_err(start_error_response)?;
    Ok(StatusCode::OK)
}

fn start_error_response(error: StartError) -> (StatusCode, String) {
    match error {
        StartError::InvalidSourceName => (StatusCode::BAD_REQUEST, "source name contains a null byte".to_owned()),
        StartError::AlreadyRunning => (StatusCode::CONFLICT, "a monitor is already running".to_owned()),
        StartError::ReplayBufferUnavailable(message) => (StatusCode::PRECONDITION_FAILED, message),
        StartError::MatcherUnavailable => (StatusCode::INTERNAL_SERVER_ERROR, "failed to init matcher".to_owned()),
        StartError::CaptureUnavailable => {
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to create capture context".to_owned())
        }
        StartError::WorkerUnavailable => {
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to spawn monitor thread".to_owned())
        }
    }
}

#[axum::debug_handler]
pub async fn handle_stop(State(state): State<AppState>) -> Result<impl IntoResponse> {
    if !state.monitor.stop(StopReason::UserStopped).await {
        return Err((StatusCode::CONFLICT, "no monitor is running").into());
    }
    Ok(StatusCode::OK)
}
