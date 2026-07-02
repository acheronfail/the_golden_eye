use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};

use crate::http::AppState;
use crate::settings::{default_completed_output_path, default_failed_output_path};

#[axum::debug_handler]
pub async fn handle_start(State(_): State<AppState>) -> Result<impl IntoResponse> {
    unsafe {
        tracing::info!("starting recording");
        crate::ffi::obs_frontend_recording_start();
    }

    Ok(StatusCode::OK)
}

#[axum::debug_handler]
pub async fn handle_stop(State(_): State<AppState>) -> Result<impl IntoResponse> {
    unsafe {
        tracing::info!("stopping recording");
        crate::ffi::obs_frontend_recording_stop();
    }

    Ok(StatusCode::OK)
}

/// Replay-buffer status. `enabled` reflects the OBS profile checkbox;
/// `available` whether OBS has a replay-buffer output object for the current
/// output settings; `active` whether it is currently running. Mirrored by the
/// frontend's `ReplayBufferStatus`.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayBufferStatus {
    enabled: bool,
    available: bool,
    active: bool,
    max_seconds: Option<u64>,
    output_directory: Option<String>,
    default_completed_output_path: Option<String>,
    default_failed_output_path: Option<String>,
}

/// Reports whether the replay buffer is enabled/available in OBS (and running),
/// so the frontend can prompt the user before starting a session.
#[axum::debug_handler]
pub async fn handle_replay_status(State(_): State<AppState>) -> Json<ReplayBufferStatus> {
    let output_directory = crate::recording::replay_buffer_output_directory();
    let default_completed_output_path =
        output_directory.as_deref().map(default_completed_output_path).map(|path| path.to_string_lossy().into_owned());
    let default_failed_output_path = default_completed_output_path.as_deref().and_then(default_failed_output_path);

    Json(ReplayBufferStatus {
        enabled: crate::recording::replay_buffer_enabled(),
        available: crate::recording::replay_buffer_available(),
        active: crate::recording::replay_buffer_active(),
        max_seconds: crate::recording::replay_buffer_max_seconds(),
        output_directory: output_directory.map(|path| path.to_string_lossy().into_owned()),
        default_completed_output_path,
        default_failed_output_path,
    })
}
