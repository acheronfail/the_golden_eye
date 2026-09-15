use std::ffi::CString;

use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Result};
use serde::Deserialize;

use crate::capture_tools::screenshot::encode_bmp_bgra;

#[derive(Deserialize)]
pub struct Params {
    /// Name of the OBS source to capture, as reported by `/api/v1/sources`.
    source: String,
}

pub async fn handler(Query(params): Query<Params>) -> Result<impl IntoResponse> {
    let source_name =
        CString::new(params.source).map_err(|_| (StatusCode::BAD_REQUEST, "source name contains a null byte"))?;

    let frame = crate::obs::capture_source_frame(&source_name)
        .ok_or((StatusCode::BAD_REQUEST, "could not capture source frame"))?;

    let bytes = encode_bmp_bgra(frame.bytes(), frame.width(), frame.height()).map_err(|err| {
        tracing::error!("failed to encode screenshot: {err}");
        (StatusCode::INTERNAL_SERVER_ERROR, "failed to encode screenshot")
    })?;

    Ok(([(header::CONTENT_TYPE, "image/bmp")], bytes))
}
