use std::ffi::CString;
use std::io::Cursor;

use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Result};
use serde::Deserialize;

use crate::timer::PhaseTimer;

#[derive(Deserialize)]
pub struct Params {
    /// Name of the OBS source to capture, as reported by `/api/v1/sources`.
    source: String,
    /// Language of the templates to match against (e.g. `en`, `jp`).
    lang: String,
}

pub async fn handler(Query(params): Query<Params>) -> Result<impl IntoResponse> {
    let source_name =
        CString::new(params.source).map_err(|_| (StatusCode::BAD_REQUEST, "source name contains a null byte"))?;

    let mut timer = PhaseTimer::new();
    let matcher = match std::env::var("GE_CV_TEMPLATE_DIR") {
        Ok(template_dir) => Some(crate::cv::CvMatcher::new(&params.lang, &template_dir).unwrap()),
        Err(_) => {
            tracing::error!("Please set GE_CV_TEMPLATE_DIR in the environment");
            None
        }
    };
    timer.lap("matcher init");

    // Render the source into a BGRA buffer owned by the C side.
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let frame = unsafe { crate::ffi::ge_obs_get_source_frame(source_name.as_ptr(), &mut width, &mut height) };
    if frame.is_null() {
        return Err((StatusCode::NOT_FOUND, "could not capture source frame").into());
    }

    timer.lap("obs frame");

    if let Some(matcher) = matcher {
        let result = matcher.match_level_from_raw_bytes(frame, width, height);
        timer.lap("cv match");
        tracing::info!(?result, "match result");
    }

    // Encode while we still own the buffer, then hand it straight back to the
    // C allocator regardless of whether encoding succeeded.
    let result = encode_bmp(frame, width, height);
    unsafe { crate::ffi::free(frame.cast()) };

    let bytes = result.map_err(|err| {
        tracing::error!("failed to encode screenshot: {err}");
        (StatusCode::INTERNAL_SERVER_ERROR, "failed to encode screenshot")
    })?;

    Ok(([(header::CONTENT_TYPE, "image/bmp")], bytes))
}

/// Copies a `width * height` BGRA buffer into a BMP-encoded byte vector.
///
/// # Safety
/// `frame` must point to at least `width * height * 4` valid bytes.
fn encode_bmp(frame: *const u8, width: u32, height: u32) -> std::io::Result<Vec<u8>> {
    let pixels = unsafe { std::slice::from_raw_parts(frame, (width * height * 4) as usize) };

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
