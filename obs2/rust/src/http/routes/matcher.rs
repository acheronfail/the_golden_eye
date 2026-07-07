use std::ffi::CString;

use axum::Json;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

use crate::cv::LevelMatch;
use crate::timer::PhaseTimer;

#[derive(Deserialize)]
pub struct Params {
    /// Name of the OBS source to capture, as reported by `/api/v1/sources`.
    source: String,
    /// Language of the templates to match against (e.g. `en`, `jp`).
    lang: String,
}

#[derive(Serialize)]
pub struct MatchResponse {
    #[serde(rename = "match")]
    level_match: LevelMatch,
    /// The annotated captured frame, PNG-encoded and base64-encoded.
    #[serde(rename = "imageData")]
    image_data: String,
    #[serde(rename = "imageMime")]
    image_mime: &'static str,
    #[serde(rename = "diagnosticsEnabled")]
    diagnostics_enabled: bool,
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
    let matcher = matcher.with_diagnostics(true);
    let diagnostics_enabled = matcher.diagnostics_enabled();
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

    let bytes =
        crate::cv_annotate::annotated_png_from_bgra(&frame_bytes, width, height, &level_match).map_err(|err| {
            tracing::error!("failed to encode annotated match image: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to encode annotated match image")
        })?;
    timer.lap("annotated image");

    Ok(Json(MatchResponse {
        level_match,
        image_data: BASE64.encode(bytes),
        image_mime: "image/png",
        diagnostics_enabled,
    }))
}
