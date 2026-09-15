use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::Deserialize;

use crate::app::AppState;
use crate::youtube_uploads::YoutubeStatus;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadRequest {
    path: String,
    datetime_local: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenYoutubeRequest {
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgetUploadRequest {
    path: String,
}

#[axum::debug_handler]
pub async fn handle_status(State(state): State<AppState>) -> Json<YoutubeStatus> {
    Json(state.youtube.status())
}

#[axum::debug_handler]
pub async fn handle_connect(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let status = state.youtube.connect(&state.event_tx).await.map_err(connect_error_response)?;
    Ok((StatusCode::OK, Json(status)))
}

fn connect_error_response(error: crate::youtube_uploads::ConnectError) -> axum::response::Response {
    use crate::youtube_uploads::ConnectError;
    let (status, message) = match error {
        ConnectError::Disabled => (StatusCode::NOT_FOUND, "YouTube uploads are not enabled in this build"),
        ConnectError::Unconfigured => (StatusCode::PRECONDITION_FAILED, "YouTube OAuth client is not configured"),
        ConnectError::Cancelled => (StatusCode::BAD_REQUEST, "OAuth flow was cancelled"),
        ConnectError::ExchangeFailed => (StatusCode::BAD_REQUEST, "YouTube OAuth failed"),
        ConnectError::BrowserUnavailable => (StatusCode::INTERNAL_SERVER_ERROR, "failed to open browser"),
    };
    (status, message).into_response()
}

#[axum::debug_handler]
pub async fn handle_open(Json(req): Json<OpenYoutubeRequest>) -> Result<impl IntoResponse> {
    if !is_allowed_youtube_url(&req.url) {
        return Err((StatusCode::BAD_REQUEST, "not a supported YouTube URL").into_response().into());
    }

    tokio::task::spawn_blocking(move || crate::desktop::browser::open_url(&req.url))
        .await
        .map_err(|err| {
            tracing::error!("YouTube browser open task failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "browser open failed").into_response()
        })?
        .map_err(|err| {
            tracing::error!("YouTube browser open failed: {err:#}");
            (StatusCode::BAD_REQUEST, "browser open failed").into_response()
        })?;

    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
pub async fn handle_cancel(State(state): State<AppState>) -> Result<impl IntoResponse> {
    // Dropping the pending sender makes the waiting connect request resolve as
    // cancelled, freeing the UI to offer Connect again.
    state.youtube.cancel_connect().await;
    Ok((StatusCode::OK, Json(state.youtube.status())))
}

#[axum::debug_handler]
pub async fn handle_disconnect(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let status = state.youtube.disconnect(&state.event_tx).map_err(|error| {
        use crate::youtube_uploads::DisconnectError;
        match error {
            DisconnectError::Disabled => (StatusCode::NOT_FOUND, "YouTube uploads are not enabled in this build"),
            DisconnectError::DeleteFailed => (StatusCode::INTERNAL_SERVER_ERROR, "failed to disconnect YouTube"),
        }
    })?;
    Ok((StatusCode::OK, Json(status)))
}

#[axum::debug_handler]
pub async fn handle_forget(
    State(state): State<AppState>,
    Json(req): Json<ForgetUploadRequest>,
) -> Result<impl IntoResponse> {
    let status = state.youtube.forget(&state.settings, &req.path).map_err(|error| {
        use crate::youtube_uploads::ForgetError;
        match error {
            ForgetError::Disabled => {
                (StatusCode::NOT_FOUND, "YouTube uploads are not enabled in this build").into_response()
            }
            ForgetError::Path(error) => super::runs::run_error_response(error),
            ForgetError::DeleteFailed => {
                (StatusCode::INTERNAL_SERVER_ERROR, "failed to forget YouTube upload history").into_response()
            }
        }
    })?;
    Ok((StatusCode::OK, Json(status)))
}

#[axum::debug_handler]
pub async fn handle_upload(State(state): State<AppState>, Json(req): Json<UploadRequest>) -> Result<impl IntoResponse> {
    let (upload, created) = state
        .youtube
        .queue_upload(&state.settings, &req.path, req.datetime_local.as_deref(), state.event_tx.clone())
        .map_err(queue_error_response)?;
    Ok((if created { StatusCode::ACCEPTED } else { StatusCode::OK }, Json(upload)))
}

fn queue_error_response(error: crate::youtube_uploads::QueueError) -> axum::response::Response {
    use crate::youtube_uploads::QueueError;
    let (status, message) = match error {
        QueueError::Disabled => (StatusCode::NOT_FOUND, "YouTube uploads are not enabled in this build"),
        QueueError::Disconnected => (StatusCode::PRECONDITION_FAILED, "YouTube is not connected"),
        QueueError::Path(error) => return super::runs::run_error_response(error),
        QueueError::Index => (StatusCode::INTERNAL_SERVER_ERROR, "could not index run clip"),
        QueueError::Metadata => (StatusCode::BAD_REQUEST, "could not read run clip metadata"),
        QueueError::File => (StatusCode::BAD_REQUEST, "could not read run clip file"),
    };
    (status, message).into_response()
}

fn is_allowed_youtube_url(url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };
    if url.scheme() != "https" {
        return false;
    }
    matches!(url.host_str(), Some("youtu.be") | Some("www.youtube.com") | Some("youtube.com"))
}
