use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};

use crate::app::AppState;
use crate::obs::{ReplayBufferStatus, current_replay_buffer_status};

#[axum::debug_handler]
pub async fn handle_start(State(_): State<AppState>) -> Result<impl IntoResponse> {
    tracing::info!("starting recording");
    crate::obs::start_recording();

    Ok(StatusCode::OK)
}

#[axum::debug_handler]
pub async fn handle_stop(State(_): State<AppState>) -> Result<impl IntoResponse> {
    tracing::info!("stopping recording");
    crate::obs::stop_recording();

    Ok(StatusCode::OK)
}

#[axum::debug_handler]
pub async fn handle_replay_status(State(state): State<AppState>) -> Json<ReplayBufferStatus> {
    let status = current_replay_buffer_status();
    state.snapshot.set_replay_buffer(status.clone());
    Json(status)
}
