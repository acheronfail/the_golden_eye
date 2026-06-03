use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};

use crate::http::AppState;

#[axum::debug_handler]
pub async fn handle_start(State(_): State<AppState>) -> Result<impl IntoResponse> {
    unsafe {
        tracing::info!("starting recording");
        crate::obs_ffi::obs_frontend_recording_start();
    }

    Ok(StatusCode::OK)
}

#[axum::debug_handler]
pub async fn handle_stop(State(_): State<AppState>) -> Result<impl IntoResponse> {
    unsafe {
        tracing::info!("stopping recording");
        crate::obs_ffi::obs_frontend_recording_stop();
    }

    Ok(StatusCode::OK)
}
