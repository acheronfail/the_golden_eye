use opencv::core::{self, Mat, Rect, Size, ToInputArray};
use opencv::imgproc;
use opencv::prelude::*;
use serde::Serialize;

use crate::config::dbg_cv;
use crate::match_result::MatchRegion;
use crate::template_matching::MatchRect;
use crate::{ActivePictureRegion, Result};

// Frames taller than this are downscaled to it before matching. 480 matches the
// composite/HDMI captures handled accurately, bounding match time. Exposed so
// live capture can downscale up front (GPU), making this internal step a no-op.
pub const WORK_HEIGHT: i32 = 480;

// GoldenEye renders 4:3, but some HDMI converters stretch it to 16:9, so glyphs
// come out too wide for the single-scale matcher. The always-on manilla folder
// (known proportions) calibrates a horizontal squish. See `calibrate_frame`.
pub(super) const TARGET_ASPECT: f64 = 4.0 / 3.0;
// The manilla folder's width:height measures ~1.20-1.26 on clean 4:3 captures. A
// folder wider than this signals a horizontally stretched picture; the threshold
// sits between that native band and the ~1.66 a 16:9-stretched folder measures.
pub(super) const FOLDER_STRETCH_ASPECT: f64 = 1.45;
// Height the frame is downscaled to for the one-off folder measurement. The
// folder's aspect is scale-invariant, so a small frame measures it just as well
// and keeps the cold-frame calibration cheap.
const FOLDER_DETECT_HEIGHT: i32 = 360;
// A column/row counts as part of the folder when at least this fraction of it
// is warm (manilla) pixels. High enough to ignore the stray warm specks in a
// thumbnail photo, low enough to include the folder's softer rounded edges.
const FOLDER_PROJ_FRAC: f64 = 0.25;
// A detected warm region smaller than this fraction of the frame (either axis)
// is rejected as not-a-folder -- gameplay can have warm patches, but the menu
// folder always fills most of the frame.
const FOLDER_MIN_FRAC: f64 = 0.40;
// An edge row/column below this mean brightness counts as a transport-frame bar.
// Real bars sit near zero, below GoldenEye's visible background texture.
const BAR_BRIGHTNESS: f64 = 24.0;
// Extent [first, last] of non-bar content along one axis. A fully dark frame
// yields the complete axis because it carries no usable boundary evidence.
fn content_extent(gray: &Mat, dimension: i32, length: i32, bar_brightness: f64) -> Result<(i32, i32)> {
    if length <= 0 {
        return Ok((0, 0));
    }
    let mut means = Mat::default();
    core::reduce(gray, &mut means, dimension, core::REDUCE_AVG, core::CV_64F)?;
    let means = means.data_typed::<f64>()?;

    let mut first = 0;
    while first < length && means[first as usize] < bar_brightness {
        first += 1;
    }
    if first >= length {
        return Ok((0, length - 1));
    }
    let mut last = length - 1;
    while last > first && means[last as usize] < bar_brightness {
        last -= 1;
    }
    Ok((first, last))
}

pub(super) fn detect_active_picture(gray: &Mat) -> Result<Rect> {
    let (left, right) = content_extent(gray, 0, gray.cols(), BAR_BRIGHTNESS)?;
    let (top, bottom) = content_extent(gray, 1, gray.rows(), BAR_BRIGHTNESS)?;
    Ok(Rect::new(left, top, (right - left + 1).max(1), (bottom - top + 1).max(1)))
}

// First and last index along `dim` (0 = columns, 1 = rows) of `mask` where the
// mean (a 0..255 fraction of set pixels) exceeds `frac`. Returns (-1, -1) when
// no line clears the bar. `mask` is an 8-bit 0/255 image.
fn first_last_above(mask: &Mat, dim: i32, frac: f64) -> Result<(i32, i32)> {
    let mut reduced = Mat::default();
    core::reduce(mask, &mut reduced, dim, core::REDUCE_AVG, core::CV_64F)?;
    let data = reduced.data_typed::<f64>()?;
    let threshold = frac * 255.0;
    let mut lo = -1i32;
    let mut hi = -1i32;
    for (i, &v) in data.iter().enumerate() {
        if v > threshold {
            if lo < 0 {
                lo = i as i32;
            }
            hi = i as i32;
        }
    }
    Ok((lo, hi))
}

// Measures the manilla folder's width:height in a `w`x`h` BGRA frame, or `None`
// when no folder-like region is present. The folder is the large warm block,
// isolated by a colour+brightness mask; the frame is downscaled first (cheap).
#[derive(Clone, Copy)]
pub(super) struct FolderDetection {
    pub(super) aspect: f64,
    pub(super) rect: Rect,
}

fn scale_detected_folder_rect(extent: Rect, src: Size, detect: Size) -> Rect {
    let (src_w, src_h) = (src.width, src.height);
    let (detect_w, detect_h) = (detect.width, detect.height);
    let x0 = extent.x;
    let y0 = extent.y;
    let x1 = extent.x + extent.width - 1;
    let y1 = extent.y + extent.height - 1;
    let x = ((x0 as f64 * src_w as f64 / detect_w as f64).floor() as i32).clamp(0, src_w.saturating_sub(1));
    let y = ((y0 as f64 * src_h as f64 / detect_h as f64).floor() as i32).clamp(0, src_h.saturating_sub(1));
    let x2 = (((x1 + 1) as f64 * src_w as f64 / detect_w as f64).ceil() as i32).clamp(x + 1, src_w);
    let y2 = (((y1 + 1) as f64 * src_h as f64 / detect_h as f64).ceil() as i32).clamp(y + 1, src_h);
    Rect::new(x, y, x2 - x, y2 - y)
}

pub(super) fn detect_folder_aspect(bgra_frame: &impl ToInputArray, w: i32, h: i32) -> Result<Option<FolderDetection>> {
    if w <= 0 || h <= 0 {
        return Ok(None);
    }
    let dh = FOLDER_DETECT_HEIGHT.min(h);
    let dw = (((w as f64) * (dh as f64 / h as f64)).round() as i32).max(1);
    // Downscale the BGRA frame directly (no full-resolution colour conversion).
    let mut small = Mat::default();
    imgproc::resize(bgra_frame, &mut small, Size::new(dw, dh), 0.0, 0.0, imgproc::INTER_AREA)?;

    let mut channels: core::Vector<Mat> = core::Vector::new();
    core::split(&small, &mut channels)?;
    let b = channels.get(0)?;
    let r = channels.get(2)?;
    let mut gray = Mat::default();
    imgproc::cvt_color_def(&small, &mut gray, imgproc::COLOR_BGRA2GRAY)?;

    // Warm = red clearly above blue (manilla, not the green background) AND
    // bright. Each predicate is a binary mask; AND them into the folder mask.
    let mut warm = Mat::default();
    {
        let mut rb = Mat::default();
        core::subtract(&r, &b, &mut rb, &core::no_array(), -1)?;
        // r - b > 15 (THRESH_BINARY keeps values strictly above the threshold).
        imgproc::threshold(&rb, &mut warm, 15.0, 255.0, imgproc::THRESH_BINARY)?;
    }
    {
        let mut bright = Mat::default();
        imgproc::threshold(&gray, &mut bright, 120.0, 255.0, imgproc::THRESH_BINARY)?;
        let mut combined = Mat::default();
        core::bitwise_and(&warm, &bright, &mut combined, &core::no_array())?;
        warm = combined;
    }

    let (x0, x1) = first_last_above(&warm, 0, FOLDER_PROJ_FRAC)?;
    let (y0, y1) = first_last_above(&warm, 1, FOLDER_PROJ_FRAC)?;
    if x0 < 0 || y0 < 0 {
        return Ok(None);
    }
    let fw = (x1 - x0 + 1) as f64;
    let fh = (y1 - y0 + 1) as f64;
    // Reject a stray warm patch: the menu folder always fills most of the frame.
    if fw < dw as f64 * FOLDER_MIN_FRAC || fh < dh as f64 * FOLDER_MIN_FRAC {
        return Ok(None);
    }
    let aspect = fw / fh;
    let rect =
        scale_detected_folder_rect(Rect::new(x0, y0, x1 - x0 + 1, y1 - y0 + 1), Size::new(w, h), Size::new(dw, dh));
    dbg_cv!("[folder] box {fw}x{fh} on {dw}x{dh} aspect={aspect:.3}");
    Ok(Some(FolderDetection { aspect, rect }))
}

// Clamps `rect` to the frame, returning None if it falls entirely outside.
pub(super) fn clamp_rect(rect: Rect, cols: i32, rows: i32) -> Option<Rect> {
    let x = rect.x.clamp(0, cols);
    let y = rect.y.clamp(0, rows);
    let w = (rect.x + rect.width).min(cols) - x;
    let h = (rect.y + rect.height).min(rows) - y;
    if w >= 2 && h >= 2 { Some(Rect::new(x, y, w, h)) } else { None }
}

// The visible game pixels, independent of whether matching needs correction.
#[derive(Clone, Copy)]
pub(super) struct ActivePicture {
    pub(super) rect: Rect,
}

impl ActivePicture {
    fn full(width: i32, height: i32) -> Self {
        Self { rect: Rect::new(0, 0, width, height) }
    }

    pub(super) fn detect(gray: &Mat) -> Result<Self> {
        Ok(Self { rect: detect_active_picture(gray)? })
    }

    pub(super) fn region(self) -> ActivePictureRegion {
        ActivePictureRegion {
            x: self.rect.x.max(0) as u32,
            y: self.rect.y.max(0) as u32,
            width: self.rect.width.max(1) as u32,
            height: self.rect.height.max(1) as u32,
        }
    }
}

// Geometry applied only to the image used by level matching.
#[derive(Clone, Copy)]
pub(super) struct LevelGeometry {
    pub(super) crop_x: i32,
    pub(super) crop_w: i32,
    pub(super) target_w: i32,
}

impl LevelGeometry {
    pub(super) fn identity(width: i32) -> Self {
        Self { crop_x: 0, crop_w: width, target_w: width }
    }

    fn is_identity(self, source_width: i32) -> bool {
        self.crop_x == 0 && self.crop_w == source_width && self.target_w == source_width
    }

    fn apply(self, gray: &Mat) -> Result<Mat> {
        if self.is_identity(gray.cols()) {
            return gray.try_clone();
        }
        let window = gray.roi(Rect::new(self.crop_x, 0, self.crop_w, gray.rows()))?;
        let mut corrected = Mat::default();
        imgproc::resize(
            &window,
            &mut corrected,
            Size::new(self.target_w.max(1), gray.rows()),
            0.0,
            0.0,
            imgproc::INTER_AREA,
        )?;
        Ok(corrected)
    }
}

// Picture bounds and matcher correction learned from one folder frame.
#[derive(Clone, Copy)]
pub(super) struct FrameCalibration {
    pub(super) src_w: i32,
    pub(super) src_h: i32,
    pub(super) active_picture: ActivePicture,
    pub(super) level_geometry: LevelGeometry,
    pub(super) folder_rect: Option<Rect>,
}

// The learned aspect correction as a source-relative capture transform: the 4:3
// sub-rectangle (fractions in [0,1]) and its target aspect. The monitor feeds it
// to the capture layer so the GPU crops+un-stretches future frames in one pass.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct CaptureRegion {
    pub crop_x: f32,
    pub crop_y: f32,
    pub crop_w: f32,
    pub crop_h: f32,
    // Width:height the cropped rectangle should be resized to (always 4:3 here).
    pub out_aspect: f32,
}

impl FrameCalibration {
    pub(super) fn uncalibrated(src_w: i32, src_h: i32) -> Self {
        Self {
            src_w,
            src_h,
            active_picture: ActivePicture::full(src_w, src_h),
            level_geometry: LevelGeometry::identity(src_w),
            folder_rect: None,
        }
    }

    pub(super) fn capture_region(&self) -> CaptureRegion {
        let geometry = self.level_geometry;
        CaptureRegion {
            crop_x: geometry.crop_x as f32 / self.src_w as f32,
            crop_y: 0.0,
            crop_w: geometry.crop_w as f32 / self.src_w as f32,
            crop_h: 1.0,
            out_aspect: geometry.target_w as f32 / self.src_h as f32,
        }
    }

    pub(super) fn is_identity(&self) -> bool {
        self.level_geometry.is_identity(self.src_w)
    }

    pub(super) fn apply(&self, gray: &Mat) -> Result<Mat> {
        self.level_geometry.apply(gray)
    }
}

pub(super) struct RegionMapper {
    calib: FrameCalibration,
    corrected_w: i32,
    corrected_h: i32,
    work_w: i32,
    work_h: i32,
}

impl RegionMapper {
    pub(super) fn from_frames(calib: FrameCalibration, corrected: &Mat, work: &Mat) -> Self {
        RegionMapper {
            calib,
            corrected_w: corrected.cols(),
            corrected_h: corrected.rows(),
            work_w: work.cols(),
            work_h: work.rows(),
        }
    }

    pub(super) fn corrected_to_source(&self, r: MatchRect) -> MatchRegion {
        let geometry = self.calib.level_geometry;
        let x = geometry.crop_x as f64 + r.x as f64 * geometry.crop_w as f64 / geometry.target_w as f64;
        let w = r.w as f64 * geometry.crop_w as f64 / geometry.target_w as f64;
        MatchRegion {
            label: String::new(),
            x: x.round() as i32,
            y: r.y,
            w: w.round().max(1.0) as i32,
            h: r.h.max(1),
            score: r.score,
        }
    }

    pub(super) fn work_to_source(&self, r: MatchRect) -> MatchRegion {
        let corrected = MatchRect {
            x: (r.x as f64 * self.corrected_w as f64 / self.work_w as f64).round() as i32,
            y: (r.y as f64 * self.corrected_h as f64 / self.work_h as f64).round() as i32,
            w: (r.w as f64 * self.corrected_w as f64 / self.work_w as f64).round().max(1.0) as i32,
            h: (r.h as f64 * self.corrected_h as f64 / self.work_h as f64).round().max(1.0) as i32,
            score: r.score,
        };
        self.corrected_to_source(corrected)
    }
}

pub(super) fn fractional_rect(cols: i32, rows: i32, region: (f64, f64, f64, f64)) -> Rect {
    let (rx, ry, rw, rh) = region;
    let x0 = (cols as f64 * rx) as i32;
    let y0 = (rows as f64 * ry) as i32;
    let w = ((cols as f64 * rw) as i32).min(cols - x0).max(1);
    let h = ((rows as f64 * rh) as i32).min(rows - y0).max(1);
    Rect::new(x0, y0, w, h)
}
