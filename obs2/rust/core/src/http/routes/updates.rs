use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::plugin_updates::PluginUpdate;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenUpdateRequest {
    release_url: String,
}

#[axum::debug_handler]
pub async fn handle_open(Json(req): Json<OpenUpdateRequest>) -> Result<impl IntoResponse> {
    tokio::task::spawn_blocking(move || crate::plugin_updates::open_release_url(&req.release_url))
        .await
        .map_err(|err| {
            tracing::error!("update release browser task failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "browser open failed").into_response()
        })?
        .map_err(|err| {
            tracing::error!("update release browser open failed: {err:#}");
            (StatusCode::BAD_REQUEST, "browser open failed").into_response()
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Applies the currently-staged update if it's safe right now. Staging happens
/// automatically in the background (see `updates.rs`); this only decides
/// *whether to apply* it (auto-update opt-in or an explicit "apply now").
#[axum::debug_handler]
pub async fn handle_apply_now(State(state): State<AppState>) -> Result<impl IntoResponse> {
    state.updates.apply_now().map_err(|error| {
        use crate::plugin_updates::ApplyError;
        match error {
            ApplyError::NothingStaged => (StatusCode::NOT_FOUND, "no update is currently staged"),
            ApplyError::ActivityInProgress => {
                (StatusCode::CONFLICT, "cannot apply an update while monitoring or recording is active")
            }
        }
    })?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckNowResponse {
    update: Option<PluginUpdate>,
}

/// Checks for an update now, bypassing the configured interval. Progress is
/// published through the retained app snapshot for every connected frontend.
#[axum::debug_handler]
pub async fn handle_check_now(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let update = state.updates.clone().check_for_updates_now().await.map_err(|err| {
        tracing::error!("manual update check failed: {err:#}");
        (StatusCode::INTERNAL_SERVER_ERROR, "update check failed").into_response()
    })?;
    Ok(Json(CheckNowResponse { update }))
}

/// Downloads, verifies, and stages the latest release now, blocking until ready
/// (or failure). Used by explicit "Download now" actions when auto-update is off;
/// apply afterward via `POST /api/v1/updates/apply`. Returns 404 if up to date.
#[axum::debug_handler]
pub async fn handle_download_now(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let result = state.updates.clone().download_and_stage_latest().await.map_err(|err| {
        tracing::error!("manual update download failed: {err:#}");
        (StatusCode::INTERNAL_SERVER_ERROR, "update download failed").into_response()
    })?;
    match result {
        crate::plugin_updates::DownloadUpdateResult::Staged => Ok(StatusCode::NO_CONTENT),
        crate::plugin_updates::DownloadUpdateResult::UpToDate => {
            Err((StatusCode::NOT_FOUND, "no newer release is available to download").into_response().into())
        }
        crate::plugin_updates::DownloadUpdateResult::ManualInstallRequired => {
            Err((StatusCode::CONFLICT, "this update requires a manual installation").into_response().into())
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatusResponse {
    #[serde(flatten)]
    status: crate::plugin_updates::UpdateStatus,
}

/// The authoritative update lifecycle state also published in app snapshots.
#[axum::debug_handler]
pub async fn handle_status(State(state): State<AppState>) -> Json<UpdateStatusResponse> {
    Json(UpdateStatusResponse { status: state.updates.status() })
}
