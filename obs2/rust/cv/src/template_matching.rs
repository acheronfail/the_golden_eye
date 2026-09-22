use opencv::core::{self, Mat, Rect, Size, ToInputArray};
use opencv::prelude::*;
use opencv::{imgcodecs, imgproc};

use crate::Result;
use crate::config::dbg_cv;
use crate::parallel::par_map;

// Short difficulty labels can correlate slightly better with the prefix of a
// longer value. Prefer the wider colocated match inside this narrow margin.
const LABEL_WIDER_TIE_MARGIN: f64 = 0.04;
const HEADER_ROW_TOLERANCE: f64 = 0.25;

// The templates are authored from a capture whose visible frame is this tall.
// The stats overlay scales with the frame, so a source captured at a different
// height needs the templates resized by (frame_height / REFERENCE_HEIGHT).
const REFERENCE_HEIGHT: f64 = 1080.0;
// Multipliers searched around the resolution-implied scale. Deriving it from the
// frame height keeps the search cheap and avoids wrong-scale false matches. 1.0
// is tried first; the best-fit global scale is reused for every other template.
const SCALE_MULTIPLIERS: [f64; 7] = [1.0, 0.95, 1.05, 0.90, 1.10, 0.85, 1.15];

// Candidate template scales for a frame `frame_height` pixels tall.
pub(super) fn candidate_scales(frame_height: i32) -> Vec<f64> {
    let base = frame_height as f64 / REFERENCE_HEIGHT;
    SCALE_MULTIPLIERS.iter().map(|m| base * m).collect()
}

// Templates are authored pixel-sharp, but real composite/HDMI sources blur the
// glyphs. Softening templates with a small Gaussian keeps correlation high on
// blurry input and costs almost nothing on sharp input (tiny kernel).
const TEMPLATE_BLUR_KSIZE: i32 = 3;

#[derive(Clone, Copy, Debug)]
pub(super) struct Detection {
    pub(super) x: i32,     // left edge in the frame
    pub(super) y: i32,     // top edge in the frame
    pub(super) w: i32,     // glyph width at the matched scale
    pub(super) h: i32,     // glyph height at the matched scale
    pub(super) score: f64, // correlation score
    pub(super) value: i32, // digit value 0-9 (unused for the colon)
}

#[derive(Clone, Copy, Debug)]
pub(super) struct MatchRect {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) w: i32,
    pub(super) h: i32,
    pub(super) score: f64,
}

impl MatchRect {
    pub(super) fn offset(self, dx: i32, dy: i32) -> Self {
        MatchRect { x: self.x + dx, y: self.y + dy, ..self }
    }

    pub(super) fn from_rect(rect: Rect) -> Self {
        MatchRect { x: rect.x, y: rect.y, w: rect.width, h: rect.height, score: 0.0 }
    }
}

// Loads "<dir>/<lang>-<name>.png" as a single-channel (grayscale) template.
// Returns an empty Mat when the file is missing or unreadable.
pub(super) fn load_template(dir: &str, lang: &str, name: &str) -> Result<Mat> {
    // `dir` may be a canonicalized path. On Windows that is verbatim (`\\?\`),
    // where '/' is a literal char, so `format!("{dir}/...")` would silently miss
    // every template. `Path::join` uses the native separator and stays correct.
    let path = std::path::Path::new(dir).join(format!("{lang}-{name}.png"));
    // Skip missing templates without an OpenCV warning; an empty Mat means
    // "no template" to every caller.
    if !path.exists() {
        return Ok(Mat::default());
    }
    // imread returns an empty Mat (not an error) when the file is unreadable.
    imgcodecs::imread(&path.to_string_lossy(), imgcodecs::IMREAD_GRAYSCALE)
}

// Softens `tmpl` with a small Gaussian so sharp emulator-authored templates
// correlate against blurry composite/HDMI sources. The kernel is clamped to the
// template size (and forced odd) so tiny glyphs at small scales stay valid.
fn blurred(tmpl: &Mat) -> Result<Mat> {
    if tmpl.empty() {
        return tmpl.try_clone();
    }
    let max_k = tmpl.cols().min(tmpl.rows());
    let mut k = TEMPLATE_BLUR_KSIZE.min(max_k);
    if k % 2 == 0 {
        k -= 1;
    }
    if k < 3 {
        return tmpl.try_clone();
    }
    let mut out = Mat::default();
    imgproc::gaussian_blur_def(tmpl, &mut out, Size::new(k, k), 0.0)?;
    Ok(out)
}

// Returns `tmpl` resized by `scale` then softened to match blurry sources.
pub(super) fn scaled(tmpl: &Mat, scale: f64) -> Result<Mat> {
    // A missing template loads as an empty Mat; resizing it would assert,
    // so pass it through untouched.
    if tmpl.empty() {
        return tmpl.try_clone();
    }
    if scale == 1.0 {
        return blurred(tmpl);
    }
    let w = ((tmpl.cols() as f64 * scale).round() as i32).max(1);
    let h = ((tmpl.rows() as f64 * scale).round() as i32).max(1);
    let mut out = Mat::default();
    let interp = if scale < 1.0 { imgproc::INTER_AREA } else { imgproc::INTER_LINEAR };
    imgproc::resize(tmpl, &mut out, Size::new(w, h), 0.0, 0.0, interp)?;
    blurred(&out)
}

// Best single-location match of `tmpl` against `frame`.
pub(super) fn best_match(frame: &(impl MatTraitConst + ToInputArray), tmpl: &Mat) -> Result<Option<MatchRect>> {
    if tmpl.empty() || tmpl.rows() > frame.rows() || tmpl.cols() > frame.cols() {
        return Ok(None);
    }
    let mut result = Mat::default();
    imgproc::match_template(frame, tmpl, &mut result, imgproc::TM_CCOEFF_NORMED, &core::no_array())?;
    let mut max_val = 0f64;
    let mut max_loc = core::Point::default();
    core::min_max_loc(&result, None, Some(&mut max_val), None, Some(&mut max_loc), &core::no_array())?;
    Ok(Some(MatchRect { x: max_loc.x, y: max_loc.y, w: tmpl.cols(), h: tmpl.rows(), score: max_val }))
}

// Picks the strongest template at `scale`, optionally resolving near-ties in
// favour of a wider candidate at the same position.
pub(super) fn best_label_match_with_wider_ties(
    frame: &(impl MatTraitConst + ToInputArray),
    templates: &[Mat],
    scale: f64,
    threshold: f64,
    prefer_wider_ties: bool,
) -> Result<Option<(i32, MatchRect)>> {
    // Own the (small) region so the per-template closures can share a `&Mat`
    // across the scoped threads, then match every label template in parallel.
    let frame = frame.try_clone()?;
    let frame = &frame;
    let scores: Vec<Result<Option<MatchRect>>> =
        par_map(templates.len(), |i| best_match(frame, &scaled(&templates[i], scale)?));

    let mut candidates = Vec::new();
    for (i, r) in scores.into_iter().enumerate() {
        let Some(r) = r? else { continue };
        let s = r.score;
        dbg_cv!("[label] idx={} scale={scale:.3} score={s:.3} x={} y={}", i + 1, r.x, r.y);
        candidates.push((i as i32 + 1, r));
    }
    let Some(mut best) =
        candidates.iter().copied().filter(|(_, r)| r.score >= threshold).max_by(|a, b| a.1.score.total_cmp(&b.1.score))
    else {
        return Ok(None);
    };
    let raw_best_score = best.1.score;
    if prefer_wider_ties {
        for candidate in candidates {
            let tolerance = ((candidate.1.h.max(best.1.h) as f64) * 0.25).round().max(1.0) as i32;
            if candidate.1.w > best.1.w
                && candidate.1.score >= threshold
                && candidate.1.score >= raw_best_score - LABEL_WIDER_TIE_MARGIN
                && (candidate.1.x - best.1.x).abs() <= tolerance
                && (candidate.1.y - best.1.y).abs() <= tolerance
            {
                best = candidate;
            }
        }
    }
    Ok(Some(best))
}

// Matches a header label only around its expected row centre. The three header
// rows move together, so the mission colon is a more reliable anchor than a
// broad scan that can latch onto similar Japanese glyphs on another row.
pub(super) fn best_label_near_row(
    region: &(impl MatTraitConst + ToInputArray),
    templates: &[Mat],
    scale: f64,
    threshold: f64,
    row_cy: i32,
    anchor_h: i32,
    prefer_wider_ties: bool,
) -> Result<Option<(i32, MatchRect)>> {
    let template_h = templates
        .iter()
        .filter(|t| !t.empty())
        .map(|t| (t.rows() as f64 * scale).round() as i32)
        .max()
        .unwrap_or(anchor_h)
        .max(1);
    let tolerance = (anchor_h as f64 * HEADER_ROW_TOLERANCE).round() as i32;
    let y0 = row_cy - template_h / 2 - tolerance;
    let y1 = row_cy + (template_h + 1) / 2 + tolerance;
    let y0 = y0.clamp(0, region.rows());
    let y1 = y1.clamp(0, region.rows());
    if y1 - y0 < 2 {
        return Ok(None);
    }
    let band = region.roi(Rect::new(0, y0, region.cols(), y1 - y0))?;
    Ok(best_label_match_with_wider_ties(&band, templates, scale, threshold, prefer_wider_ties)?
        .map(|(idx, r)| (idx, r.offset(0, y0))))
}

// Like `best_label_match`, but also sweeps `scales`. Used to recover the true
// overlay scale when the scale implied by the frame height is wrong.
pub(super) fn best_label_match_over_scales(
    frame: &(impl MatTraitConst + ToInputArray),
    templates: &[Mat],
    scales: &[f64],
    threshold: f64,
) -> Result<(i32, f64, Option<MatchRect>)> {
    let mut best = -1;
    let mut best_score_v = threshold;
    let mut best_scale = scales.first().copied().unwrap_or(1.0);
    let mut best_rect = None;
    for &scale in scales {
        for (i, t) in templates.iter().enumerate() {
            let Some(r) = best_match(frame, &scaled(t, scale)?)? else { continue };
            let s = r.score;
            if s >= best_score_v {
                best_score_v = s;
                best = i as i32 + 1;
                best_scale = scale;
                best_rect = Some(r);
            }
        }
    }
    Ok((best, best_scale, best_rect))
}

// Collects every location where `tmpl` matches `frame` above `threshold`.
pub(super) fn collect_detections(
    frame: &(impl MatTraitConst + ToInputArray),
    tmpl: &Mat,
    threshold: f64,
    value: i32,
    out: &mut Vec<Detection>,
) -> Result<()> {
    if tmpl.empty() || tmpl.rows() > frame.rows() || tmpl.cols() > frame.cols() {
        return Ok(());
    }
    let mut result = Mat::default();
    imgproc::match_template(frame, tmpl, &mut result, imgproc::TM_CCOEFF_NORMED, &core::no_array())?;
    let cols = result.cols();
    let rows = result.rows();
    let data = result.data_typed::<f32>()?;
    let w = tmpl.cols();
    for y in 0..rows {
        let row = &data[(y * cols) as usize..((y + 1) * cols) as usize];
        for x in 0..cols {
            let score = row[x as usize];
            if score as f64 >= threshold {
                out.push(Detection { x, y, w, h: tmpl.rows(), score: score as f64, value });
            }
        }
    }
    Ok(())
}

// Greedy non-maximum suppression: keeps the strongest detection in each
// neighbourhood, dropping weaker ones whose centre lies within
// (cell_w * frac, cell_h * frac) of an already-kept detection.
pub(super) fn suppress(mut dets: Vec<Detection>, cell_w: i32, cell_h: i32, frac: f64) -> Vec<Detection> {
    dets.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    let mut kept: Vec<Detection> = Vec::new();
    for d in dets {
        let overlaps = kept.iter().any(|k| {
            (d.x - k.x).abs() < (cell_w as f64 * frac) as i32 && (d.y - k.y).abs() < (cell_h as f64 * frac) as i32
        });
        if !overlaps {
            kept.push(d);
        }
    }
    kept
}
