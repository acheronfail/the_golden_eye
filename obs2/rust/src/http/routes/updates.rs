use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::{Deserialize, Serialize};

use crate::http::AppState;
use crate::updates::PluginUpdate;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenUpdateRequest {
    release_url: String,
}

#[axum::debug_handler]
pub async fn handle_open(Json(req): Json<OpenUpdateRequest>) -> Result<impl IntoResponse> {
    tokio::task::spawn_blocking(move || crate::updates::open_release_url(&req.release_url))
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
    if !crate::update_apply::has_staged_update() {
        return Err((StatusCode::NOT_FOUND, "no update is currently staged").into_response().into());
    }
    if !crate::update_apply::is_safe_to_apply(&state) {
        return Err((StatusCode::CONFLICT, "cannot apply an update while monitoring or recording is active")
            .into_response()
            .into());
    }

    crate::update_apply::trigger_apply();
    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckNowResponse {
    update: Option<PluginUpdate>,
}

/// Checks for an update now, bypassing the configured check interval (see
/// `updates::check_for_updates_now`). Staging happens in the background; poll
/// `GET /api/v1/updates/status` to see when it's ready to apply.
#[axum::debug_handler]
pub async fn handle_check_now(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let update = crate::updates::check_for_updates_now(state).await.map_err(|err| {
        tracing::error!("manual update check failed: {err:#}");
        (StatusCode::INTERNAL_SERVER_ERROR, "update check failed").into_response()
    })?;
    Ok(Json(CheckNowResponse { update }))
}

/// Downloads, verifies, and stages the latest release now, blocking until ready
/// (or failure). Used by explicit "Download now" actions when auto-update is off;
/// apply afterward via `POST /api/v1/updates/apply`. Returns 404 if up to date.
#[axum::debug_handler]
pub async fn handle_download_now() -> Result<impl IntoResponse> {
    let staged = crate::updates::download_and_stage_latest().await.map_err(|err| {
        tracing::error!("manual update download failed: {err:#}");
        (StatusCode::INTERNAL_SERVER_ERROR, "update download failed").into_response()
    })?;
    if !staged {
        return Err((StatusCode::NOT_FOUND, "no newer release is available to download").into_response().into());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatusResponse {
    staged: bool,
}

/// Whether a verified update is currently staged and ready to apply -- the
/// same check `handle_apply_now` itself uses, exposed so the UI can show
/// "check now" vs "apply now" without guessing.
#[axum::debug_handler]
pub async fn handle_status() -> Json<UpdateStatusResponse> {
    Json(UpdateStatusResponse { staged: crate::update_apply::has_staged_update() })
}
