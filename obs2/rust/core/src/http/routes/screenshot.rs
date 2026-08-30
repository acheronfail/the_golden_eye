use std::ffi::CString;
use std::io::Cursor;

use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Result};
use serde::Deserialize;

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

/// Copies a `width * height` BGRA slice into a BMP-encoded byte vector.
pub(crate) fn encode_bmp_bgra(pixels: &[u8], width: u32, height: u32) -> std::io::Result<Vec<u8>> {
    let mut image = bmp::Image::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            // Source is BGRA; drop the alpha channel.
            image.set_pixel(x, y, bmp::Pixel::new(pixels[i + 2], pixels[i + 1], pixels[i]));
        }
    }

    let mut out = Cursor::new(Vec::new());
    image.to_writer(&mut out)?;
    Ok(out.into_inner())
}

#[cfg(test)]
#[path = "screenshot_test.rs"]
mod screenshot_test;
