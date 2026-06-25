// Standalone CLI for exercising the GoldenEye level matcher outside of OBS.
//
//   test_match path/to/screenshot.png [templates_dir]
//
// Loads the given image, converts it to the BGRA layout the plugin feeds the
// matcher, runs the matcher, and prints the match result to stdout. `lang`
// is read from the GE_LANG environment variable (default: "en") and
// `templates_dir` defaults to the cv_templates/ directory that ships
// alongside obs2/.
//
// This is a Rust port of obs2/test_match.cpp + obs2/cv_wrapper.cpp, using the
// `opencv` crate instead of binding to OpenCV directly.

use opencv::core::{self, Mat, Point, Rect, Size, ToInputArray};
use opencv::prelude::*;
use opencv::{Result, imgcodecs, imgproc};

use crate::timer::PhaseTimer;

// Correlation needed to accept a mission/part/difficulty label match.
const LABEL_THRESHOLD: f64 = 0.70;
// A mission match this strong means the current scale is essentially exact, so
// the remaining (resolution-recovery) scales cannot improve on it and the scale
// search can stop early.
const STRONG_LABEL: f64 = 0.90;

// Fraction of the frame searched for the mission/part/difficulty labels. They
// always sit in the upper-left of the stats overlay, so only the top 50% /
// left 60% needs to be searched.
const LABEL_REGION_W: f64 = 0.60;
const LABEL_REGION_H: f64 = 0.50;

// Region searched for the time colons: they sit in the upper part of the
// stats table. Anchoring the (full-frame) colon search to this box also
// discards label colons ("Time:", "Accuracy:") and lower stat rows for free.
const COLON_REGION_X: f64 = 0.25;
const COLON_REGION_W: f64 = 0.50;
const COLON_REGION_Y: f64 = 0.50;
const COLON_REGION_H: f64 = 0.20;
// Correlation needed to accept an individual digit/colon glyph.
const GLYPH_THRESHOLD: f64 = 0.78;
// The pre-label gate should only admit very likely time colons. The full
// extractor still uses GLYPH_THRESHOLD once a time frame is suspected.
const TIME_GATE_COLON_THRESHOLD: f64 = 0.88;
const TIME_GATE_SLOT_THRESHOLD: f64 = 0.89;
const TIME_GATE_STRONG_COLON: f64 = 0.90;

// Candidate scales applied to the templates when locating the stats overlay.
// The templates are authored at the user's native capture resolution, so 1.0 is
// tried first and is the common case; the remaining scales let matching survive
// when the source is captured at a different resolution. A single global scale
// (the one that best fits the mission label) is then reused for every other
// template so the glyphs stay crisply aligned.
const SCALES: [f64; 10] = [1.0, 0.9, 1.1, 0.8, 1.2, 0.75, 1.33, 0.67, 1.5, 0.6];

#[derive(Clone, Copy, Debug)]
struct Detection {
    x: i32,     // left edge in the frame
    y: i32,     // top edge in the frame
    w: i32,     // glyph width at the matched scale
    score: f64, // correlation score
    value: i32, // digit value 0-9 (unused for the colon)
}

// A time recovered from the screen, kept with its position so the final array
// can be ordered top-to-bottom then left-to-right.
struct FoundTime {
    y: i32,
    x: i32,
    seconds: i32,
}

struct FoundMission {
    mission: i32,
    score: f64,
}

#[derive(Debug)]
pub struct LevelMatch {
    pub mission: i32,
    pub part: i32,
    pub difficulty: i32,
    pub times: Vec<i32>,
    pub runtime_ms: f64,
}

// Loads "<dir>/<lang>-<name>.png" as a single-channel (grayscale) template.
// Returns an empty Mat when the file is missing or unreadable.
fn load_template(dir: &str, lang: &str, name: &str) -> Result<Mat> {
    let path = format!("{dir}/{lang}-{name}.png");
    // imread returns an empty Mat (not an error) when the file is missing.
    imgcodecs::imread(&path, imgcodecs::IMREAD_GRAYSCALE)
}

// Returns `tmpl` resized by `scale` (or a clone of the original when scale == 1.0).
fn scaled(tmpl: &Mat, scale: f64) -> Result<Mat> {
    if scale == 1.0 {
        return tmpl.try_clone();
    }
    let w = ((tmpl.cols() as f64 * scale).round() as i32).max(1);
    let h = ((tmpl.rows() as f64 * scale).round() as i32).max(1);
    let mut out = Mat::default();
    let interp = if scale < 1.0 { imgproc::INTER_AREA } else { imgproc::INTER_LINEAR };
    imgproc::resize(tmpl, &mut out, Size::new(w, h), 0.0, 0.0, interp)?;
    Ok(out)
}

// Best single-location match of `tmpl` against `frame`. Returns the peak
// correlation, or -1.0 if the template does not fit inside the frame.
fn best_score(frame: &(impl MatTraitConst + ToInputArray), tmpl: &Mat) -> Result<f64> {
    if tmpl.empty() || tmpl.rows() > frame.rows() || tmpl.cols() > frame.cols() {
        return Ok(-1.0);
    }
    let mut result = Mat::default();
    imgproc::match_template(frame, tmpl, &mut result, imgproc::TM_CCOEFF_NORMED, &core::no_array())?;
    let mut max_val = 0f64;
    core::min_max_loc(&result, None, Some(&mut max_val), None, None, &core::no_array())?;
    Ok(max_val)
}

// Picks the highest-scoring template from `templates` (matched at `scale`).
// Returns the 1-based index of the winner, or -1 when none clears the
// threshold.
fn best_label(
    frame: &(impl MatTraitConst + ToInputArray),
    templates: &[Mat],
    scale: f64,
    threshold: f64,
) -> Result<i32> {
    let mut best = -1;
    let mut best_score_v = threshold;
    for (i, t) in templates.iter().enumerate() {
        let s = best_score(frame, &scaled(t, scale)?)?;
        if s >= best_score_v {
            best_score_v = s;
            best = i as i32 + 1;
        }
    }
    Ok(best)
}

// Collects every location where `tmpl` matches `frame` above `threshold`.
fn collect_detections(
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
                out.push(Detection { x, y, w, score: score as f64, value });
            }
        }
    }
    Ok(())
}

// Greedy non-maximum suppression: keeps the strongest detection in each
// neighbourhood, dropping weaker ones whose centre lies within
// (cell_w * frac, cell_h * frac) of an already-kept detection.
fn suppress(mut dets: Vec<Detection>, cell_w: i32, cell_h: i32, frac: f64) -> Vec<Detection> {
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

fn best_digit_near(
    frame: &(impl MatTraitConst + ToInputArray),
    digit_tmpls: &[Mat],
    expected_x: i32,
    center_y: i32,
    digit_w: i32,
    digit_h: i32,
) -> Result<Option<Detection>> {
    let pad_x = (digit_w as f64 * 0.7).round() as i32;
    let pad_y = (digit_h as f64 * 0.35).round() as i32;
    let x0 = (expected_x - pad_x).max(0);
    let y0 = (center_y - digit_h / 2 - pad_y).max(0);
    let x1 = (expected_x + digit_w + pad_x).min(frame.cols());
    let y1 = (center_y + digit_h / 2 + pad_y).min(frame.rows());
    if x1 <= x0 || y1 <= y0 {
        return Ok(None);
    }

    let roi = frame.roi(Rect::new(x0, y0, x1 - x0, y1 - y0))?;
    let mut best: Option<Detection> = None;
    for (value, tmpl) in digit_tmpls.iter().enumerate().take(10) {
        if tmpl.empty() || tmpl.rows() > roi.rows() || tmpl.cols() > roi.cols() {
            continue;
        }

        let mut matched = Mat::default();
        imgproc::match_template(&roi, tmpl, &mut matched, imgproc::TM_CCOEFF_NORMED, &core::no_array())?;
        let mut max_val = 0f64;
        let mut max_loc = Point::default();
        core::min_max_loc(&matched, None, Some(&mut max_val), None, Some(&mut max_loc), &core::no_array())?;
        if max_val >= GLYPH_THRESHOLD && best.map(|d| max_val > d.score).unwrap_or(true) {
            best = Some(Detection {
                x: x0 + max_loc.x,
                y: y0 + max_loc.y,
                w: tmpl.cols(),
                score: max_val,
                value: value as i32,
            });
        }
    }

    Ok(best)
}

fn count_time_colons(
    frame: &(impl MatTraitConst + ToInputArray),
    colon_tmpl: &Mat,
    threshold: f64,
) -> Result<(usize, f64)> {
    if colon_tmpl.empty() {
        return Ok((0, -1.0));
    }

    let colon_x0 = (frame.cols() as f64 * COLON_REGION_X) as i32;
    let colon_y0 = (frame.rows() as f64 * COLON_REGION_Y) as i32;
    let colon_region = frame.roi(Rect::new(
        colon_x0,
        colon_y0,
        (frame.cols() as f64 * COLON_REGION_W) as i32,
        (frame.rows() as f64 * COLON_REGION_H) as i32,
    ))?;
    let mut colons = Vec::new();
    collect_detections(&colon_region, colon_tmpl, threshold, 0, &mut colons)?;
    let colons = suppress(colons, colon_tmpl.cols(), colon_tmpl.rows(), 0.5);
    let max_score = colons.iter().map(|d| d.score).fold(-1.0, f64::max);
    Ok((colons.len(), max_score))
}

// Finds a mission number (1-9) by anchoring on ':' in the label region and
// taking the strongest single digit immediately to its left on the same line.
fn find_mission_from_colons(
    label_region: &(impl MatTraitConst + ToInputArray),
    colon_tmpl: &Mat,
    digit_tmpls: &[Mat],
) -> Result<FoundMission> {
    let none = FoundMission { mission: -1, score: -1.0 };
    if colon_tmpl.empty() || digit_tmpls.len() < 10 {
        return Ok(none);
    }

    let mut digit_width_sum = 0;
    for v in 1..=9 {
        if digit_tmpls[v].empty() {
            return Ok(none);
        }
        digit_width_sum += digit_tmpls[v].cols();
    }
    let digit_w = (digit_width_sum / 9).max(1);
    let digit_h = digit_tmpls[1].rows();
    let colon_w = colon_tmpl.cols();
    let colon_h = colon_tmpl.rows();

    let mut colons = Vec::new();
    collect_detections(label_region, colon_tmpl, GLYPH_THRESHOLD, 0, &mut colons)?;
    let colons = suppress(colons, colon_w, colon_h, 0.5);

    let mut best = FoundMission { mission: -1, score: -1.0 };
    let band_pad_x = digit_w * 2;
    let band_pad_y = digit_h;

    for colon in &colons {
        let x0 = (colon.x - band_pad_x).max(0);
        let y0 = (colon.y - band_pad_y).max(0);
        let x1 = (colon.x + (colon_w / 2).max(1)).min(label_region.cols());
        let y1 = (colon.y + colon_h + band_pad_y).min(label_region.rows());
        if x1 <= x0 || y1 <= y0 {
            continue;
        }

        let roi = label_region.roi(Rect::new(x0, y0, x1 - x0, y1 - y0))?;
        let colon_center_y = colon.y as f64 + colon_h as f64 / 2.0;
        for v in 1..=9 {
            let mut per_value = Vec::new();
            collect_detections(&roi, &digit_tmpls[v], GLYPH_THRESHOLD, v as i32, &mut per_value)?;
            for mut d in per_value {
                d.x += x0;
                d.y += y0;
                if ((d.y as f64 + digit_h as f64 / 2.0) - colon_center_y).abs() >= digit_h as f64 * 0.35 {
                    continue;
                }
                if (d.x + d.w) as f64 > colon.x as f64 + colon_w as f64 * 0.7 {
                    continue;
                }
                let adj_left = (colon.x - (d.x + d.w)) as f64;
                if adj_left < -(digit_w as f64) * 0.4 || adj_left > digit_w as f64 * 0.6 {
                    continue;
                }
                if d.score >= best.score {
                    best = FoundMission { mission: v as i32, score: d.score };
                }
            }
        }
    }

    Ok(best)
}

// Recovers all complete "mm:ss" readings in the stats time area. This is kept
// separate from label matching so non-time frames can be rejected before paying
// for mission/part/difficulty template scans.
fn find_times_slot(
    frame: &(impl MatTraitConst + ToInputArray),
    colon_tmpl: &Mat,
    digit_tmpls: &[Mat],
    colon_threshold: f64,
) -> Result<Vec<FoundTime>> {
    if colon_tmpl.empty() || digit_tmpls.len() < 10 {
        return Ok(Vec::new());
    }

    let mut digit_width_sum = 0;
    for t in digit_tmpls.iter().take(10) {
        if t.empty() {
            return Ok(Vec::new());
        }
        digit_width_sum += t.cols();
    }

    let colon_w = colon_tmpl.cols();
    let colon_h = colon_tmpl.rows();
    let digit_w = digit_width_sum / 10; // representative digit width
    let digit_h = digit_tmpls[0].rows();

    let colon_x0 = (frame.cols() as f64 * COLON_REGION_X) as i32;
    let colon_y0 = (frame.rows() as f64 * COLON_REGION_Y) as i32;
    let colon_region = frame.roi(Rect::new(
        colon_x0,
        colon_y0,
        (frame.cols() as f64 * COLON_REGION_W) as i32,
        (frame.rows() as f64 * COLON_REGION_H) as i32,
    ))?;
    let mut colons = Vec::new();
    collect_detections(&colon_region, colon_tmpl, colon_threshold, 0, &mut colons)?;
    // Offset back into frame coordinates.
    for c in &mut colons {
        c.x += colon_x0;
        c.y += colon_y0;
    }
    let colons = suppress(colons, colon_w, colon_h, 0.5);

    // Assemble "mm:ss" readings: a valid time is two digits immediately to the
    // left of a colon and two immediately to its right, all on the same line.
    let mut times: Vec<FoundTime> = Vec::new();
    for colon in &colons {
        let colon_center_y = colon.y + colon_h / 2;
        let l0 = best_digit_near(frame, digit_tmpls, colon.x - digit_w, colon_center_y, digit_w, digit_h)?;
        let l1 = best_digit_near(frame, digit_tmpls, colon.x - digit_w * 2, colon_center_y, digit_w, digit_h)?;
        let r0 = best_digit_near(frame, digit_tmpls, colon.x + colon_w, colon_center_y, digit_w, digit_h)?;
        let r1 = best_digit_near(frame, digit_tmpls, colon.x + colon_w + digit_w, colon_center_y, digit_w, digit_h)?;
        if cfg!(debug_assertions) {
            eprintln!("[ge_cv debug] colon at ({},{}) slots={:?}", colon.x, colon.y, [l1, l0, r0, r1]);
        }
        let (Some(l1), Some(l0), Some(r0), Some(r1)) = (l1, l0, r0, r1) else {
            continue;
        };

        // Spacing checks (in digit-width fractions): the inner digits must hug
        // the colon and the outer digits must abut the inner ones.
        let adj_right = (r0.x - (colon.x + colon_w)) as f64;
        let adj_left = (colon.x - (l0.x + l0.w)) as f64;
        let gap_right = (r1.x - (r0.x + r0.w)) as f64;
        let gap_left = (l0.x - (l1.x + l1.w)) as f64;
        if adj_right < -(digit_w as f64) * 0.4
            || adj_right > digit_w as f64 * 0.6
            || adj_left < -(digit_w as f64) * 0.4
            || adj_left > digit_w as f64 * 0.6
            || gap_right.abs() > digit_w as f64 * 0.6
            || gap_left.abs() > digit_w as f64 * 0.6
        {
            continue;
        }

        let minutes = l1.value * 10 + l0.value;
        let seconds = r0.value * 10 + r1.value;
        let total_seconds = minutes * 60 + seconds;
        if cfg!(debug_assertions) {
            eprintln!(
                "[ge_cv debug] accepted time {}:{} from colon ({},{}) minutes={} seconds={} total={}",
                l1.value, l0.value, colon.x, colon.y, minutes, seconds, total_seconds,
            );
        }

        // It's impossible for a level to last longer than 0x3ff seconds (limited by Goldeneye's save format).
        if total_seconds < 0x3ff {
            times.push(FoundTime { y: colon.y, x: colon.x, seconds: total_seconds });
        }
    }

    // Order top-to-bottom (bucketed by line) then left-to-right.
    let line_bucket = digit_h as f64 * 0.5;
    times.sort_by(|a, b| {
        let ra = (a.y as f64 / line_bucket).round() as i32;
        let rb = (b.y as f64 / line_bucket).round() as i32;
        if ra != rb { ra.cmp(&rb) } else { a.x.cmp(&b.x) }
    });

    Ok(times)
}

fn find_times_band(
    frame: &(impl MatTraitConst + ToInputArray),
    colon_tmpl: &Mat,
    digit_tmpls: &[Mat],
) -> Result<Vec<FoundTime>> {
    if colon_tmpl.empty() || digit_tmpls.len() < 10 {
        return Ok(Vec::new());
    }

    let mut digit_width_sum = 0;
    for t in digit_tmpls.iter().take(10) {
        if t.empty() {
            return Ok(Vec::new());
        }
        digit_width_sum += t.cols();
    }

    let colon_w = colon_tmpl.cols();
    let colon_h = colon_tmpl.rows();
    let digit_w = digit_width_sum / 10;
    let digit_h = digit_tmpls[0].rows();

    let colon_x0 = (frame.cols() as f64 * COLON_REGION_X) as i32;
    let colon_y0 = (frame.rows() as f64 * COLON_REGION_Y) as i32;
    let colon_region = frame.roi(Rect::new(
        colon_x0,
        colon_y0,
        (frame.cols() as f64 * COLON_REGION_W) as i32,
        (frame.rows() as f64 * COLON_REGION_H) as i32,
    ))?;
    let mut colons = Vec::new();
    collect_detections(&colon_region, colon_tmpl, GLYPH_THRESHOLD, 0, &mut colons)?;
    for c in &mut colons {
        c.x += colon_x0;
        c.y += colon_y0;
    }
    let colons = suppress(colons, colon_w, colon_h, 0.5);

    let band_pad_x = digit_w * 3;
    let band_pad_y = digit_h;
    let mut digits = Vec::new();
    for colon in &colons {
        let x0 = (colon.x - band_pad_x).max(0);
        let y0 = (colon.y - band_pad_y).max(0);
        let x1 = (colon.x + colon_w + band_pad_x).min(frame.cols());
        let y1 = (colon.y + colon_h + band_pad_y).min(frame.rows());
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let roi = frame.roi(Rect::new(x0, y0, x1 - x0, y1 - y0))?;
        for v in 0..10 {
            let mut bucket = Vec::new();
            collect_detections(&roi, &digit_tmpls[v], GLYPH_THRESHOLD, v as i32, &mut bucket)?;
            for mut d in bucket {
                d.x += x0;
                d.y += y0;
                digits.push(d);
            }
        }
    }
    let digits = suppress(digits, digit_w, digit_h, 0.5);

    let mut times: Vec<FoundTime> = Vec::new();
    for colon in &colons {
        let colon_center_y = colon.y as f64 + colon_h as f64 / 2.0;

        let mut right: Vec<Detection> = Vec::new();
        let mut left: Vec<Detection> = Vec::new();
        for d in &digits {
            if ((d.y as f64 + digit_h as f64 / 2.0) - colon_center_y).abs() >= digit_h as f64 * 0.35 {
                continue;
            }
            if d.x as f64 >= colon.x as f64 + colon_w as f64 * 0.3 {
                right.push(*d);
            } else if (d.x + d.w) as f64 <= colon.x as f64 + colon_w as f64 * 0.7 {
                left.push(*d);
            }
        }
        if right.len() < 2 || left.len() < 2 {
            continue;
        }
        right.sort_by(|a, b| a.x.cmp(&b.x));
        left.sort_by(|a, b| b.x.cmp(&a.x));

        let r0 = right[0];
        let r1 = right[1];
        let l0 = left[0];
        let l1 = left[1];

        let adj_right = (r0.x - (colon.x + colon_w)) as f64;
        let adj_left = (colon.x - (l0.x + l0.w)) as f64;
        let gap_right = (r1.x - (r0.x + r0.w)) as f64;
        let gap_left = (l0.x - (l1.x + l1.w)) as f64;
        if adj_right < -(digit_w as f64) * 0.4
            || adj_right > digit_w as f64 * 0.6
            || adj_left < -(digit_w as f64) * 0.4
            || adj_left > digit_w as f64 * 0.6
            || gap_right.abs() > digit_w as f64 * 0.6
            || gap_left.abs() > digit_w as f64 * 0.6
        {
            continue;
        }

        let minutes = l1.value * 10 + l0.value;
        let seconds = r0.value * 10 + r1.value;
        let total_seconds = minutes * 60 + seconds;
        if total_seconds < 0x3ff {
            times.push(FoundTime { y: colon.y, x: colon.x, seconds: total_seconds });
        }
    }

    let line_bucket = digit_h as f64 * 0.5;
    times.sort_by(|a, b| {
        let ra = (a.y as f64 / line_bucket).round() as i32;
        let rb = (b.y as f64 / line_bucket).round() as i32;
        if ra != rb { ra.cmp(&rb) } else { a.x.cmp(&b.x) }
    });

    Ok(times)
}

// Matches the GoldenEye level-stats overlay in a single BGRA frame against the
// template PNGs in `templates_dir`. Mirrors ge_cv_match_level().
pub fn match_level(bgra_frame: &impl ToInputArray, lang: &str, templates_dir: &str) -> Result<LevelMatch> {
    CvMatcher::new(lang, templates_dir)?.match_level_from_bgra_frame(bgra_frame)
}

pub struct CvMatcher {
    parts: Vec<Mat>,
    diffs: Vec<Mat>,
    colon: Mat,
    digits: Vec<Mat>,
}

impl CvMatcher {
    pub fn new(lang: &str, templates_dir: &str) -> Result<Self> {
        // Load the label templates.
        let mut parts = Vec::new();
        for i in 1..=5 {
            parts.push(load_template(templates_dir, lang, &format!("part{i}"))?);
        }
        let mut diffs = Vec::new();
        for i in 0..=3 {
            diffs.push(load_template(templates_dir, lang, &format!("diff{i}"))?);
        }

        // Load base glyph templates once; mission and time matching both scale from
        // these in-memory mats.
        let colon = load_template(templates_dir, lang, "colon")?;
        let mut digits = Vec::new();
        for v in 0..=9 {
            digits.push(load_template(templates_dir, lang, &format!("digit{v}"))?);
        }

        Ok(CvMatcher { parts, diffs, colon, digits })
    }

    pub fn match_level_from_raw_bytes(&self, data: *mut u8, w: u32, h: u32) -> Result<LevelMatch> {
        let total_bytes = (w * h * 4) as usize;
        let data_slice = unsafe { std::slice::from_raw_parts(data, total_bytes) };
        let bgra_frame = Mat::new_rows_cols_with_bytes::<u8>(h as i32, w as i32, data_slice)?;
        self.match_level_from_bgra_frame(&bgra_frame)
    }

    pub fn match_level_from_bgra_frame(&self, bgra_frame: &impl ToInputArray) -> Result<LevelMatch> {
        let mut result = LevelMatch { mission: -1, part: -1, difficulty: -1, times: Vec::new(), runtime_ms: 0.0 };
        let mut timer = PhaseTimer::new();

        // Convert the BGRA frame to grayscale once; every template is matched
        // against this single-channel frame.
        let mut frame = Mat::default();
        imgproc::cvt_color_def(bgra_frame, &mut frame, imgproc::COLOR_BGRA2GRAY)?;
        timer.lap("grayscale");

        let (time_colon_count, best_time_colon) = count_time_colons(&frame, &self.colon, TIME_GATE_COLON_THRESHOLD)?;
        let has_time = time_colon_count >= 2
            || best_time_colon >= TIME_GATE_STRONG_COLON
            || (time_colon_count == 1
                && best_time_colon >= TIME_GATE_SLOT_THRESHOLD
                && !find_times_slot(&frame, &self.colon, &self.digits, TIME_GATE_COLON_THRESHOLD)?.is_empty());
        timer.lap("time gate");
        if !has_time {
            result.runtime_ms = timer.start().elapsed().as_secs_f64() * 1000.0;
            return Ok(result);
        }

        // The mission/part/difficulty labels always sit in the upper-left of the
        // stats overlay, so their template matching only needs the top-left corner
        // of the frame.
        let label_region = frame.roi(Rect::new(
            0,
            0,
            (frame.cols() as f64 * LABEL_REGION_W) as i32,
            (frame.rows() as f64 * LABEL_REGION_H) as i32,
        ))?;

        // Determine the global scale from mission glyphs by anchoring on ':' in the
        // label region and selecting the strongest single digit immediately left of
        // that colon.
        let mut global_scale = 1.0;
        let mut best_mission_score = GLYPH_THRESHOLD;
        for &scale in &SCALES {
            let colon_tmpl = scaled(&self.colon, scale)?;
            let mut digit_tmpls = Vec::with_capacity(10);
            for v in 0..=9 {
                digit_tmpls.push(scaled(&self.digits[v], scale)?);
            }

            let found = find_mission_from_colons(&label_region, &colon_tmpl, &digit_tmpls)?;
            if found.score >= best_mission_score {
                best_mission_score = found.score;
                global_scale = scale;
                result.mission = found.mission;
            }
            // A near-perfect match means we have already found the right scale, so
            // the remaining scales cannot improve on it -- stop early.
            if best_mission_score >= STRONG_LABEL {
                break;
            }
        }
        timer.lap("mission scale search");

        // Remaining labels are matched at the established scale.
        result.part = best_label(&label_region, &self.parts, global_scale, LABEL_THRESHOLD)?;
        timer.lap("part labels");
        let difficulty_label = best_label(&label_region, &self.diffs, global_scale, LABEL_THRESHOLD)?;
        result.difficulty = if difficulty_label >= 0 { difficulty_label.saturating_sub(1) } else { -1 };
        timer.lap("difficulty labels");

        // Locate the digit and colon glyphs at the same scale.
        let colon_tmpl = scaled(&self.colon, global_scale)?;
        let mut digit_tmpls = Vec::with_capacity(10);
        let mut digit_width_sum = 0;
        for v in 0..=9 {
            let t = scaled(&self.digits[v], global_scale)?;
            digit_width_sum += t.cols();
            digit_tmpls.push(t);
        }
        timer.lap("load glyph templates");

        let times = if colon_tmpl.empty() || digit_width_sum == 0 {
            Vec::new()
        } else {
            find_times_band(&frame, &colon_tmpl, &digit_tmpls)?
        };
        result.times = times.into_iter().map(|t| t.seconds).collect();
        timer.lap("time assembly");

        result.runtime_ms = timer.start().elapsed().as_secs_f64() * 1000.0;

        Ok(result)
    }
}
