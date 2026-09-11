//! One-shot frame inspection, independent of the live monitor's matcher and state.
use std::ffi::CString;

use serde::Serialize;

use crate::cv::{LevelMatch, PhaseTimer};

#[derive(Debug)]
pub(crate) enum MatchError {
    EmptyImage,
    InvalidSource,
    MissingTemplates,
    MatcherUnavailable,
    InvalidImage,
    CaptureUnavailable,
    MatchFailed,
}

#[derive(Serialize, ts_rs::TS)]
#[ts(rename = "MatchSourceResponse")]
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

pub(crate) fn match_image(lang: &str, annotations: bool, body: &[u8]) -> Result<MatchResponse, MatchError> {
    if body.is_empty() {
        return Err(MatchError::EmptyImage);
    }
    let Some(template_dir) = crate::cv::template_dir() else {
        tracing::error!("CV template directory is not set");
        return Err(MatchError::MissingTemplates);
    };
    let matcher = crate::cv::CvMatcher::new(&lang, &template_dir)
        .map_err(|err| {
            tracing::error!("failed to init matcher: {err}");
            MatchError::MatcherUnavailable
        })?
        .with_diagnostics(annotations);
    let annotations_enabled = matcher.diagnostics_enabled();

    let (level_match, width, height) = matcher.match_level_from_encoded_image(&body).map_err(|err| {
        tracing::error!("failed to decode/match uploaded image: {err}");
        MatchError::InvalidImage
    })?;

    Ok(MatchResponse { level_match, annotations_enabled, frame_width: width, frame_height: height })
}

pub(crate) fn match_source(source: String, lang: &str, annotations: bool) -> Result<MatchResponse, MatchError> {
    let source_name = CString::new(source).map_err(|_| MatchError::InvalidSource)?;

    let mut timer = PhaseTimer::new();
    let Some(template_dir) = crate::cv::template_dir() else {
        tracing::error!("CV template directory is not set");
        return Err(MatchError::MissingTemplates);
    };
    let matcher = crate::cv::CvMatcher::new(&lang, &template_dir).map_err(|err| {
        tracing::error!("failed to init matcher: {err}");
        MatchError::MatcherUnavailable
    })?;
    let matcher = matcher.with_diagnostics(annotations);
    let annotations_enabled = matcher.diagnostics_enabled();
    timer.lap("matcher init");

    let frame = crate::obs::capture_source_frame(&source_name).ok_or(MatchError::CaptureUnavailable)?;

    timer.lap("obs frame");

    let level_match = matcher.match_level_from_bgra_bytes(frame.bytes(), frame.width(), frame.height());
    timer.lap("cv match");
    tracing::info!(?level_match, "match result");

    let level_match = level_match.map_err(|err| {
        tracing::error!("failed to match level: {err}");
        MatchError::MatchFailed
    })?;

    Ok(MatchResponse { level_match, annotations_enabled, frame_width: frame.width(), frame_height: frame.height() })
}
