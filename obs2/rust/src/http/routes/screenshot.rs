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

    // Render the source into a BGRA buffer owned by the C side.
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let frame = unsafe { crate::ffi::ge_obs_get_source_frame(source_name.as_ptr(), &mut width, &mut height) };
    if frame.is_null() {
        return Err((StatusCode::BAD_REQUEST, "could not capture source frame").into());
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
/// # Safety
/// `frame` must point to at least `width * height * 4` valid bytes.
fn encode_bmp(frame: *const u8, width: u32, height: u32) -> std::io::Result<Vec<u8>> {
    let pixels = unsafe { std::slice::from_raw_parts(frame, (width * height * 4) as usize) };
    encode_bmp_bgra(pixels, width, height)
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
mod tests {
    use opencv::prelude::*;
    use opencv::{core, imgcodecs};

    use super::*;

    /// The frame dump writes these BMPs and `test_match` reads them back with
    /// OpenCV's `imread`; guard that the encoded format stays decodable and keeps
    /// its dimensions and BGR pixel values (imread yields BGR from a BGRA source).
    #[test]
    fn dumped_bmp_decodes_through_imread() {
        // 2x2 BGRA: distinct colours so a channel/row swap would be caught.
        let pixels = [
            10, 20, 30, 255, 40, 50, 60, 255, // row 0
            70, 80, 90, 255, 100, 110, 120, 255, // row 1
        ];
        let bmp = encode_bmp_bgra(&pixels, 2, 2).expect("encode bmp");

        let path = std::env::temp_dir().join(format!("ge-bmp-roundtrip-{}.bmp", std::process::id()));
        std::fs::write(&path, bmp).expect("write bmp");
        let decoded = imgcodecs::imread(path.to_str().unwrap(), imgcodecs::IMREAD_COLOR).expect("imread");
        std::fs::remove_file(&path).ok();

        assert!(!decoded.empty(), "imread could not decode the dumped BMP");
        assert_eq!((decoded.cols(), decoded.rows()), (2, 2));
        // Top-left pixel round-trips its BGR bytes.
        let px = decoded.at_2d::<core::Vec3b>(0, 0).expect("pixel");
        assert_eq!([px[0], px[1], px[2]], [10, 20, 30]);
    }
}
