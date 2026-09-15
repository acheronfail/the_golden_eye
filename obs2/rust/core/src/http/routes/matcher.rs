use axum::Json;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::capture_tools::matching::{self, MatchError, MatchResponse};

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
    state.monitor.set_annotations_enabled(params.annotations);
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

pub async fn handle_upload(Query(params): Query<UploadParams>, body: Bytes) -> Result<Json<MatchResponse>> {
    matching::match_image(&params.lang, params.annotations, &body)
        .map(Json)
        .map_err(|error| match_error_response(error).into())
}

pub async fn handler(Query(params): Query<Params>) -> Result<impl IntoResponse> {
    matching::match_source(params.source, &params.lang, params.annotations)
        .map(Json)
        .map_err(|error| match_error_response(error).into())
}

fn match_error_response(error: MatchError) -> (StatusCode, &'static str) {
    match error {
        MatchError::EmptyImage => (StatusCode::BAD_REQUEST, "empty image body"),
        MatchError::InvalidSource => (StatusCode::BAD_REQUEST, "source name contains a null byte"),
        MatchError::MissingTemplates => (StatusCode::INTERNAL_SERVER_ERROR, "CV template directory is not set"),
        MatchError::MatcherUnavailable => (StatusCode::INTERNAL_SERVER_ERROR, "failed to init matcher"),
        MatchError::InvalidImage => (StatusCode::BAD_REQUEST, "could not decode the uploaded image"),
        MatchError::CaptureUnavailable => (StatusCode::NOT_FOUND, "could not capture source frame"),
        MatchError::MatchFailed => (StatusCode::INTERNAL_SERVER_ERROR, "failed to match level"),
    }
}
