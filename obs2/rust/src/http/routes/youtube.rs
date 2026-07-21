use std::time::SystemTime;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;

use crate::http::{AppEvent, AppState, PendingOAuth};
use crate::youtube::{self, YoutubeStatus};

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
    if !state.youtube.enabled() {
        return Err((StatusCode::NOT_FOUND, "YouTube uploads are not enabled in this build").into());
    }
    if !state.youtube.oauth_configured() {
        return Err((StatusCode::PRECONDITION_FAILED, "YouTube OAuth client is not configured").into());
    }

    let oauth_state = new_oauth_state();
    let auth_url = state.youtube.config().authorization_url(&oauth_state);
    let (tx, rx) = oneshot::channel();
    {
        let mut pending = state.oauth_pending.lock().await;
        *pending = Some(PendingOAuth { state: oauth_state, tx });
    }

    open_consent_page(&auth_url).await?;

    let code = rx.await.map_err(|_| (StatusCode::BAD_REQUEST, "OAuth flow was cancelled").into_response())?;
    state.youtube.exchange_code(&code).await.map_err(|err| {
        tracing::error!("YouTube OAuth exchange failed: {err:#}");
        (StatusCode::BAD_REQUEST, "YouTube OAuth failed").into_response()
    })?;

    Ok((StatusCode::OK, Json(state.youtube.status())))
}

#[axum::debug_handler]
pub async fn handle_open(Json(req): Json<OpenYoutubeRequest>) -> Result<impl IntoResponse> {
    if !is_allowed_youtube_url(&req.url) {
        return Err((StatusCode::BAD_REQUEST, "not a supported YouTube URL").into_response().into());
    }

    tokio::task::spawn_blocking(move || crate::browser::open_url(&req.url))
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
    state.oauth_pending.lock().await.take();
    Ok((StatusCode::OK, Json(state.youtube.status())))
}

#[axum::debug_handler]
pub async fn handle_disconnect(State(state): State<AppState>) -> Result<impl IntoResponse> {
    if !state.youtube.enabled() {
        return Err((StatusCode::NOT_FOUND, "YouTube uploads are not enabled in this build").into());
    }
    state.youtube.disconnect().map_err(|err| {
        tracing::error!("failed to disconnect YouTube: {err:#}");
        (StatusCode::INTERNAL_SERVER_ERROR, "failed to disconnect YouTube").into_response()
    })?;
    Ok((StatusCode::OK, Json(state.youtube.status())))
}

#[axum::debug_handler]
pub async fn handle_forget(
    State(state): State<AppState>,
    Json(req): Json<ForgetUploadRequest>,
) -> Result<impl IntoResponse> {
    if !state.youtube.enabled() {
        return Err((StatusCode::NOT_FOUND, "YouTube uploads are not enabled in this build").into());
    }
    let settings = state.settings.get_effective();
    crate::http::routes::runs::authorize_tagged_run_path(&settings, &req.path).map_err(|err| err.into_response())?;
    state.youtube.forget_for_display_path(req.path.trim()).map_err(|err| {
        tracing::error!("failed to forget YouTube upload history: {err:#}");
        (StatusCode::INTERNAL_SERVER_ERROR, "failed to forget YouTube upload history").into_response()
    })?;
    Ok((StatusCode::OK, Json(state.youtube.status())))
}

#[axum::debug_handler]
pub async fn handle_upload(State(state): State<AppState>, Json(req): Json<UploadRequest>) -> Result<impl IntoResponse> {
    if !state.youtube.enabled() {
        return Err((StatusCode::NOT_FOUND, "YouTube uploads are not enabled in this build").into());
    }
    if !state.youtube.connected() {
        return Err((StatusCode::PRECONDITION_FAILED, "YouTube is not connected").into());
    }

    let settings = state.settings.get_effective();
    let display_path = req.path.trim().to_owned();
    let path = crate::http::routes::runs::authorize_tagged_run_path(&settings, &display_path)
        .map_err(|err| err.into_response())?;
    if let Some(existing) = state.youtube.active_upload_for_display_path(&display_path) {
        return Ok((StatusCode::OK, Json(existing)));
    }

    let clip = state
        .run_catalog
        .refresh_clip(&path)
        .map_err(|err| {
            tracing::warn!(path = %path.display(), "failed to index clip before YouTube upload: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not index run clip").into_response()
        })?
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "could not read run clip metadata").into_response())?;
    let metadata = clip.metadata;
    let total_bytes = std::fs::metadata(&path)
        .map_err(|err| {
            tracing::warn!(path = %path.display(), "failed to read clip file size: {err:#}");
            (StatusCode::BAD_REQUEST, "could not read run clip file").into_response()
        })?
        .len();
    let (title, description) =
        youtube::render_youtube_metadata(&settings, &path, &metadata, req.datetime_local.as_deref());
    let status =
        state.youtube.insert_queued_upload(&path, display_path, title.clone(), description.clone(), total_bytes);
    let _ = state.event_tx.send(AppEvent::YoutubeUploadChanged { upload: status.clone() });

    let store = state.youtube.clone();
    let event_tx = state.event_tx.clone();
    let request = youtube::UploadRequest {
        upload_id: status.id.clone(),
        path: path.clone(),
        title,
        description,
        visibility: settings.youtube_visibility,
    };
    tokio::spawn(async move {
        youtube::upload_video(store, request, event_tx).await;
    });

    Ok((StatusCode::ACCEPTED, Json(status)))
}

/// Opens the Google consent page in the user's browser. Tests exercise the OAuth
/// flow without launching a real browser, so this is a no-op under `test-hooks`.
#[cfg(not(feature = "test-hooks"))]
async fn open_consent_page(auth_url: &str) -> Result<()> {
    let auth_url = auth_url.to_owned();
    tokio::task::spawn_blocking(move || crate::browser::open_url(&auth_url))
        .await
        .map_err(|err| {
            tracing::error!("failed to join browser opener task: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to open browser").into_response()
        })?
        .map_err(|err| {
            tracing::error!("failed to open YouTube OAuth URL: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to open browser").into_response()
        })?;
    Ok(())
}

#[cfg(feature = "test-hooks")]
async fn open_consent_page(_auth_url: &str) -> Result<()> {
    Ok(())
}

fn new_oauth_state() -> String {
    #[cfg(feature = "test-hooks")]
    if let Some(state) = crate::config::test_oauth_state() {
        return state;
    }
    let mut hasher = Sha256::new();
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos().to_le_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
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
