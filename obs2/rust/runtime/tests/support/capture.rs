use ge_runtime::GeCaptureRegion;
use opencv::core::{Mat, Rect, Size};
use opencv::imgproc;
use opencv::prelude::*;

use super::test_obs::Frame;

// Mirrors obs_bridge.c capture dimensions and crop rules with CPU resampling.
// Each request starts from source pixels, including after calibration.
pub fn capture_frame(source: &Frame, max_height: u32, region: Option<&GeCaptureRegion>) -> opencv::Result<Frame> {
    let pixels = Mat::from_slice(&source.bgra)?;
    let pixels = pixels.reshape(4, source.height as i32)?;
    let (rect, width, height) = if let Some(region) = region.filter(|r| r.out_width != 0 && r.out_height != 0) {
        let cx = region.crop_x.max(0.0);
        let cy = region.crop_y.max(0.0);
        let cw = (if region.crop_w <= 0.0 { 1.0 } else { region.crop_w }).min(1.0 - cx);
        let ch = (if region.crop_h <= 0.0 { 1.0 } else { region.crop_h }).min(1.0 - cy);
        let x = (cx * source.width as f32).round() as i32;
        let y = (cy * source.height as f32).round() as i32;
        let w = ((cw * source.width as f32).round() as i32).min(source.width as i32 - x);
        let h = ((ch * source.height as f32).round() as i32).min(source.height as i32 - y);
        (Rect::new(x, y, w, h), region.out_width, region.out_height)
    } else {
        let (width, height) = if max_height != 0 && source.height > max_height {
            let width = ((source.width as u64 * max_height as u64 + source.height as u64 / 2) / source.height as u64)
                .max(1) as u32;
            (width, max_height)
        } else {
            (source.width, source.height)
        };
        (Rect::new(0, 0, source.width as i32, source.height as i32), width, height)
    };
    let crop = pixels.roi(rect)?;
    let mut output = Mat::default();
    imgproc::resize(&crop, &mut output, Size::new(width as i32, height as i32), 0.0, 0.0, imgproc::INTER_AREA)?;
    Ok(Frame { width, height, bgra: output.data_bytes()?.to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stripes() -> Frame {
        let mut bgra = Vec::new();
        for _ in 0..4 {
            for value in [0, 40, 80, 120, 160, 200] {
                bgra.extend_from_slice(&[value, value, value, 255]);
            }
        }
        Frame { width: 6, height: 4, bgra }
    }

    #[test]
    fn capture_height_cap_rounds_width_and_preserves_small_sources() {
        let source = stripes();
        for cap in [0, 4, 480] {
            let frame = capture_frame(&source, cap, None).unwrap();
            assert_eq!((frame.width, frame.height), (6, 4));
            assert_eq!(frame.bgra, source.bgra);
        }
        let frame = capture_frame(&source, 3, None).unwrap();
        assert_eq!((frame.width, frame.height), (5, 3));
        assert_eq!(frame.bgra.len(), 5 * 3 * 4);
        assert!(frame.bgra.chunks_exact(4).all(|pixel| pixel[3] == 255));
    }

    #[test]
    fn calibrated_capture_crops_source_and_uses_requested_output_size() {
        let source = stripes();
        let region = GeCaptureRegion {
            crop_x: 1.0 / 3.0,
            crop_y: 0.25,
            crop_w: 1.0 / 3.0,
            crop_h: 0.5,
            out_width: 4,
            out_height: 4,
        };
        let frame = capture_frame(&source, 1, Some(&region)).unwrap();
        assert_eq!((frame.width, frame.height), (4, 4));
        for row in frame.bgra.chunks_exact(16) {
            assert_eq!(row, &[80, 80, 80, 255, 80, 80, 80, 255, 120, 120, 120, 255, 120, 120, 120, 255]);
        }
        assert_eq!(source.bgra, stripes().bgra);
    }

    #[test]
    fn capture_region_defaults_and_clamps_to_source_bounds() {
        let region =
            GeCaptureRegion { crop_x: 2.0 / 3.0, crop_y: -0.5, crop_w: 0.0, crop_h: -1.0, out_width: 2, out_height: 4 };
        let frame = capture_frame(&stripes(), 0, Some(&region)).unwrap();
        for row in frame.bgra.chunks_exact(8) {
            assert_eq!(row, &[160, 160, 160, 255, 200, 200, 200, 255]);
        }
        let disabled = GeCaptureRegion { out_width: 0, ..region };
        let frame = capture_frame(&stripes(), 2, Some(&disabled)).unwrap();
        assert_eq!((frame.width, frame.height), (3, 2));
    }
}
