use std::ffi::CString;
use std::sync::atomic::Ordering;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::{Deserialize, Serialize};

use crate::cv::LevelMatch;
use crate::http::AppState;
use crate::timer::PhaseTimer;

#[derive(Deserialize)]
pub struct Params {
    /// Name of the OBS source to capture, as reported by `/api/v1/sources`.
    source: String,
    /// Language of the templates to match against (e.g. `en`, `jp`).
    lang: String,
    /// Whether to include developer annotation sets in the match result.
    #[serde(default)]
    annotations: bool,
}

#[derive(Serialize)]
pub struct MatchResponse {
    #[serde(rename = "match")]
    level_match: LevelMatch,
    #[serde(rename = "annotationsEnabled")]
    annotations_enabled: bool,
    #[serde(rename = "frameWidth")]
    frame_width: u32,
    #[serde(rename = "frameHeight")]
    frame_height: u32,
}

#[derive(Deserialize)]
pub struct AnnotationParams {
    annotations: bool,
}

#[derive(Serialize)]
pub struct AnnotationResponse {
    #[serde(rename = "annotationsEnabled")]
    annotations_enabled: bool,
}

pub async fn handle_annotations(
    State(state): State<AppState>,
    Json(params): Json<AnnotationParams>,
) -> Json<AnnotationResponse> {
    state.monitor_annotations_enabled.store(params.annotations, Ordering::Release);
    Json(AnnotationResponse { annotations_enabled: params.annotations })
}

#[derive(Deserialize)]
pub struct UploadParams {
    /// Language of the templates to match against (e.g. `en`, `jp`).
    lang: String,
    /// Whether to include developer annotation sets in the match result.
    #[serde(default)]
    annotations: bool,
}

/// Matches an image uploaded in the request body (PNG/BMP), for the developer
/// tool's drag-and-drop frame inspector. Coordinates in the result/annotations
/// are in the uploaded image's pixel space.
pub async fn handle_upload(Query(params): Query<UploadParams>, body: Bytes) -> Result<Json<MatchResponse>> {
    if body.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "empty image body").into());
    }
    let Some(template_dir) = crate::cv::template_dir() else {
        tracing::error!("CV template directory is not set");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "CV template directory is not set").into());
    };
    let matcher = crate::cv::CvMatcher::new(&params.lang, &template_dir)
        .map_err(|err| {
            tracing::error!("failed to init matcher: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to init matcher")
        })?
        .with_diagnostics(params.annotations);
    let annotations_enabled = matcher.diagnostics_enabled();

    let (level_match, width, height) = matcher.match_level_from_encoded_image(&body).map_err(|err| {
        tracing::error!("failed to decode/match uploaded image: {err}");
        (StatusCode::BAD_REQUEST, "could not decode the uploaded image")
    })?;

    Ok(Json(MatchResponse { level_match, annotations_enabled, frame_width: width, frame_height: height }))
}

pub async fn handler(Query(params): Query<Params>) -> Result<impl IntoResponse> {
    let source_name =
        CString::new(params.source).map_err(|_| (StatusCode::BAD_REQUEST, "source name contains a null byte"))?;

    let mut timer = PhaseTimer::new();
    let Some(template_dir) = crate::cv::template_dir() else {
        tracing::error!("CV template directory is not set");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "CV template directory is not set").into());
    };
    let matcher = crate::cv::CvMatcher::new(&params.lang, &template_dir).map_err(|err| {
        tracing::error!("failed to init matcher: {err}");
        (StatusCode::INTERNAL_SERVER_ERROR, "failed to init matcher")
    })?;
    let matcher = matcher.with_diagnostics(params.annotations);
    let annotations_enabled = matcher.diagnostics_enabled();
    timer.lap("matcher init");

    // Render the source into a BGRA buffer owned by the C side.
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let frame = unsafe { crate::ffi::ge_obs_get_source_frame(source_name.as_ptr(), &mut width, &mut height) };
    if frame.is_null() {
        return Err((StatusCode::NOT_FOUND, "could not capture source frame").into());
    }

    timer.lap("obs frame");

    let frame_len = (width * height * 4) as usize;
    let frame_bytes = unsafe { std::slice::from_raw_parts(frame, frame_len).to_vec() };
    unsafe { crate::ffi::free(frame.cast()) };

    let level_match = matcher.match_level_from_bgra_bytes(&frame_bytes, width, height);
    timer.lap("cv match");
    tracing::info!(?level_match, "match result");

    let level_match = level_match.map_err(|err| {
        tracing::error!("failed to match level: {err}");
        (StatusCode::INTERNAL_SERVER_ERROR, "failed to match level")
    })?;

    Ok(Json(MatchResponse { level_match, annotations_enabled, frame_width: width, frame_height: height }))
}
