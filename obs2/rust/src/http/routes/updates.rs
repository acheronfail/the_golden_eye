use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::Deserialize;

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
