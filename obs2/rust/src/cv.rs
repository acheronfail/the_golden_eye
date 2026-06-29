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

use opencv::core::{self, Mat, Rect, Size, ToInputArray};
use opencv::prelude::*;
use opencv::{Result, imgcodecs, imgproc};

use std::sync::Mutex;

use crate::timer::PhaseTimer;

// Set GE_CV_DEBUG to dump intermediate match scores/detections to stderr.
fn dbg_on() -> bool {
    std::env::var_os("GE_CV_DEBUG").is_some()
}
macro_rules! dbg_cv {
    ($($arg:tt)*) => {
        if dbg_on() { eprintln!($($arg)*); }
    };
}

// Correlation needed to accept a mission/part/difficulty label match.
const LABEL_THRESHOLD: f64 = 0.70;

// Fraction of the frame searched for the mission/part/difficulty labels. They
// always sit in the upper-left of the stats overlay, so only the top 50% /
// left 60% needs to be searched.
const LABEL_REGION_W: f64 = 0.60;
const LABEL_REGION_H: f64 = 0.50;

// Box searched for the mission digit, as fractions of the frame. It spans the
// three header rows but excludes the level title above and the
// "PRIMARY OBJECTIVES:" / "STATISTICS:" rows below, so the anchor never latches
// onto an unrelated colon and the search stays cheap. "Mission N:" sits near the
// left margin, so the right side never holds the digit.
const MISSION_REGION_X: f64 = 0.0;
const MISSION_REGION_W: f64 = 0.40;
const MISSION_REGION_Y: f64 = 0.18;
const MISSION_REGION_H: f64 = 0.26;
// Mission-digit correlation that ends the scale sweep early. The
// resolution-implied scale is tried first and, on the native-res frame, lands
// the real digit there at ~0.95-0.97; accepting at 0.90 lets that first scale
// settle the common case in one pass. Only off-scale captures
// (letterboxed/windowed) score below this on the first scale and fall through to
// the remaining scales. Anchoring is restricted to confident colons
// ([[COLON_ANCHOR_THRESHOLD]]), so the digit found here sits on a real header
// row rather than on background texture.
const MISSION_STRONG: f64 = 0.90;

// Region searched for the time colons: they sit in the upper part of the
// stats table. The box is kept generous because the overlay does not always
// land in the same place: captures that letterbox or rescale the console
// output (composite -> HDMI converters, different capture resolutions) push
// the stats table higher and further left than a clean emulator grab. A wider
// box tolerates that drift; the "mm:ss" spacing checks downstream still reject
// label colons ("Time:", "Accuracy:") and other stray matches.
// The region must reach far enough down that a time-row colon near its lower
// edge still fits (match_template only reports a colon whose full height lands
// inside the box). That height offset scales with the source, so a bottom edge
// of ~0.62 detects the Time and Target/Best rows at every resolution yet still
// excludes the lower stat table ("Shot total:", "Head hits:"), whose colons
// only begin around 0.61+ and would need the box to reach ~0.66 to register.
const COLON_REGION_X: f64 = 0.15;
const COLON_REGION_W: f64 = 0.62;
const COLON_REGION_Y: f64 = 0.45;
const COLON_REGION_H: f64 = 0.17;

// Region searched by the entry gate for the stats-overlay header colons. Both
// the level-start (briefing) screen and the post-mission stats screen carry the
// same three left-aligned header rows ("<Difficulty>:", "Mission N:",
// "Part <roman>:"), each ending in a colon, in the upper-left of the frame.
// Counting strong colons here admits both screens (so the start screen's
// mission/part/difficulty can be read) while still rejecting busy gameplay
// frames, which lack a tidy stack of label colons in this band.
const HEADER_REGION_X: f64 = 0.08;
const HEADER_REGION_W: f64 = 0.56;
const HEADER_REGION_Y: f64 = 0.18;
const HEADER_REGION_H: f64 = 0.30;

// Screen classification. Every header screen carries a banner word one line
// below the "<Difficulty>:" / "Mission N:" / "Part <roman>:" stack
// ("PRIMARY OBJECTIVES:", "STATISTICS:", "SPECIAL OPTIONS:", "DIFFICULTY:");
// the four post-mission report screens instead carry the same "REPORT:" banner
// and are told apart by the status value one line lower ("Completed" /
// "FAILED" / "ABORTED" / "KILLED IN ACTION"). The banner sits in the upper
// band below the header; the status value sits in the band just beneath it,
// left of the per-objective result column on the right. Each template is
// matched in its band and the strongest above this threshold wins. The bands
// are kept generous so composite/HDMI overlay drift still lands the text
// inside them.
const SCREEN_THRESHOLD: f64 = 0.78;
// (x, y, w, h) as fractions of the frame.
const SCREEN_BANNER_REGION: (f64, f64, f64, f64) = (0.04, 0.39, 0.56, 0.11);
const SCREEN_STATUS_REGION: (f64, f64, f64, f64) = (0.18, 0.47, 0.48, 0.10);
// Correlation needed to accept an individual digit/colon glyph.
const GLYPH_THRESHOLD: f64 = 0.78;
// Colon correlation required to anchor a mission-number search. Higher than the
// glyph threshold: real header colons clear it easily, but it keeps the tiny
// colon template from matching background texture (each false hit is expensive,
// driving a per-colon digit search and quadratic suppression).
const COLON_ANCHOR_THRESHOLD: f64 = 0.86;
// The entry gate admits a frame only when it finds two header colons (the
// "<Difficulty>:" / "Mission N:" / "Part <roman>:" stack) AND at least one is a
// confident match. Composite/HDMI sources and window-chrome captures soften the
// glyphs, so the count threshold sits in the low 0.8s and the peak requirement
// at 0.85 -- low enough to admit a blurry windowed jp grab (whose colons top out
// near 0.87) yet high enough that busy gameplay frames, lacking a tidy colon
// stack, stay out. Non-stats frames that do slip through still read no times.
const TIME_GATE_COLON_THRESHOLD: f64 = 0.84;
const TIME_GATE_STRONG_COLON: f64 = 0.85;

// The templates are authored from a capture whose visible frame is this tall.
// The stats overlay scales with the frame, so a source captured at a different
// height needs the templates resized by (frame_height / REFERENCE_HEIGHT).
const REFERENCE_HEIGHT: f64 = 1080.0;

// Frames taller than this are downscaled to it before matching. 480 is the
// height of the composite/HDMI captures the matcher already handles accurately,
// so normalizing every source to it bounds match time without losing accuracy.
const WORK_HEIGHT: i32 = 480;

// Multipliers searched around the resolution-implied scale. Deriving the scale
// from the frame height (rather than blindly sweeping a fixed ladder) keeps the
// search cheap -- a native-resolution frame never matches tiny templates and a
// 640x480 composite grab never matches full-size ones -- and avoids the
// wrong-scale false matches a wide sweep produces. 1.0 (the implied scale) is
// tried first; the neighbours absorb overscan and letterboxing. A single global
// scale (the one that best fits the mission label) is then reused for every
// other template so the glyphs stay crisply aligned.
const SCALE_MULTIPLIERS: [f64; 7] = [1.0, 0.95, 1.05, 0.90, 1.10, 0.85, 1.15];

// Candidate template scales for a frame `frame_height` pixels tall.
fn candidate_scales(frame_height: i32) -> Vec<f64> {
    let base = frame_height as f64 / REFERENCE_HEIGHT;
    SCALE_MULTIPLIERS.iter().map(|m| base * m).collect()
}

// Templates are authored from a pixel-sharp emulator, but most real sources
// pass through composite cabling and HDMI converters that blur the glyphs.
// Softening the (already downscaled) templates with a small Gaussian closes
// that gap so the normalized correlation stays high on blurry input; it costs
// almost nothing on sharp input because the kernel is tiny.
const TEMPLATE_BLUR_KSIZE: i32 = 3;

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
    // Centre of the anchoring "Mission N:" colon, in the coordinates of the
    // region it was searched in. The vertical centre pins the difficulty row
    // (one line up) and part row (one line down); both centres let a later frame
    // re-search the mission in a tight box instead of the whole header.
    colon_cx: i32,
    colon_cy: i32,
}

// Which overlay screen a frame shows. All of these except `Levels` (the
// unimplemented mission grid) share the mission/part/difficulty header; they
// are told apart by the banner word below it ("STATISTICS:", "SPECIAL
// OPTIONS:", "DIFFICULTY:", "PRIMARY OBJECTIVES:") or, for the four
// post-mission report screens, by the status value ("Completed" / "FAILED" /
// "ABORTED" / "KILLED IN ACTION"). `Unknown` covers gameplay and anything the
// gate rejects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Unknown,
    Start,
    Stats,
    Complete,
    Failed,
    Abort,
    Kia,
    Opts007,
    Select,
}

impl Screen {
    // Strings match the `ScreenshotInfo.screen` values used by the test suite.
    pub fn as_str(self) -> &'static str {
        match self {
            Screen::Unknown => "unknown",
            Screen::Start => "start",
            Screen::Stats => "stats",
            Screen::Complete => "complete",
            Screen::Failed => "failed",
            Screen::Abort => "abort",
            Screen::Kia => "kia",
            Screen::Opts007 => "007opts",
            Screen::Select => "select",
        }
    }
}

#[derive(Debug)]
pub struct LevelMatch {
    pub screen: Screen,
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
    // Some templates are intentionally absent for a language (e.g. jp has no
    // difficulty-select banner). Skip the read in that case so OpenCV does not
    // log a spurious "can't open/read file" warning; an empty Mat means the
    // same "no template" to every caller.
    if !std::path::Path::new(&path).exists() {
        return Ok(Mat::default());
    }
    // imread returns an empty Mat (not an error) when the file is unreadable.
    imgcodecs::imread(&path, imgcodecs::IMREAD_GRAYSCALE)
}

// Softens `tmpl` in place with a small Gaussian so the sharp emulator-authored
// templates correlate against blurry composite/HDMI-converted sources. The
// kernel is clamped to the template size (and forced odd) so tiny glyphs at
// small scales stay valid.
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
fn scaled(tmpl: &Mat, scale: f64) -> Result<Mat> {
    // A missing template loads as an empty Mat (e.g. jp has no difficulty-select
    // banner); resizing it would assert, so pass it through untouched.
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
        dbg_cv!("[label] idx={} scale={scale:.3} score={s:.3}", i + 1);
        if s >= best_score_v {
            best_score_v = s;
            best = i as i32 + 1;
        }
    }
    Ok(best)
}

// Best label within a horizontal band of `region` spanning rows
// [y0, y1) (clamped to the region). The header rows are one glyph-line apart, so
// restricting the search to the band where a label sits cuts the work several
// fold versus scanning the whole upper-left corner. Returns -1 when the band is
// degenerate so the caller can fall back to a full-region search.
fn best_label_in_band(
    region: &(impl MatTraitConst + ToInputArray),
    templates: &[Mat],
    scale: f64,
    threshold: f64,
    y0: i32,
    y1: i32,
) -> Result<i32> {
    let y0 = y0.clamp(0, region.rows());
    let y1 = y1.clamp(0, region.rows());
    if y1 - y0 < 2 {
        return Ok(-1);
    }
    let band = region.roi(Rect::new(0, y0, region.cols(), y1 - y0))?;
    best_label(&band, templates, scale, threshold)
}

// Like `best_label`, but also sweeps `scales` and returns the (1-based winner,
// scale) pair with the strongest correlation. Used to recover the true overlay
// scale when the scale implied by the frame height is wrong -- e.g. a capture
// that includes window chrome or letterboxing, where the overlay fills less of
// the frame than its pixel height suggests. Whole-word labels are scale
// sensitive, so the scale that best fits one is the overlay's real scale.
fn best_label_over_scales(
    frame: &(impl MatTraitConst + ToInputArray),
    templates: &[Mat],
    scales: &[f64],
    threshold: f64,
) -> Result<(i32, f64)> {
    let mut best = -1;
    let mut best_score_v = threshold;
    let mut best_scale = scales.first().copied().unwrap_or(1.0);
    for &scale in scales {
        for (i, t) in templates.iter().enumerate() {
            let s = best_score(frame, &scaled(t, scale)?)?;
            if s >= best_score_v {
                best_score_v = s;
                best = i as i32 + 1;
                best_scale = scale;
            }
        }
    }
    Ok((best, best_scale))
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

// What the entry gate found in the header colon band: how many colons, the peak
// correlation, and the scale they matched best at. The scale is reused for the
// label searches so they are not re-run across every candidate scale.
struct HeaderColons {
    count: usize,
    peak: f64,
    scale: f64,
}

// Detects header colons inside `region` (fractional x/y/w/h of the frame),
// trying every candidate scale so the gate works regardless of capture
// resolution (the base template alone only matches an emulator-native grab).
// Returns the richest result (most colons, then highest peak) and the scale it
// occurred at, stopping early once a scale clearly clears the bar so common
// cases stay cheap.
fn detect_header_colons(
    frame: &(impl MatTraitConst + ToInputArray),
    base_colon: &Mat,
    scales: &[f64],
    threshold: f64,
    region: (f64, f64, f64, f64),
) -> Result<HeaderColons> {
    let mut best = HeaderColons { count: 0, peak: -1.0, scale: scales.first().copied().unwrap_or(1.0) };
    if base_colon.empty() {
        return Ok(best);
    }

    let (rx, ry, rw, rh) = region;
    let colon_x0 = (frame.cols() as f64 * rx) as i32;
    let colon_y0 = (frame.rows() as f64 * ry) as i32;
    let colon_region = frame.roi(Rect::new(
        colon_x0,
        colon_y0,
        (frame.cols() as f64 * rw) as i32,
        (frame.rows() as f64 * rh) as i32,
    ))?;

    for &scale in scales {
        let colon_tmpl = scaled(base_colon, scale)?;
        if colon_tmpl.empty() || colon_tmpl.rows() > colon_region.rows() || colon_tmpl.cols() > colon_region.cols() {
            continue;
        }
        let mut colons = Vec::new();
        collect_detections(&colon_region, &colon_tmpl, threshold, 0, &mut colons)?;
        let colons = suppress(colons, colon_tmpl.cols(), colon_tmpl.rows(), 0.5);
        let peak = colons.iter().map(|d| d.score).fold(-1.0, f64::max);
        // Prefer the scale that resolves the most colons (the full header
        // stack), breaking ties on peak correlation.
        if colons.len() > best.count || (colons.len() == best.count && peak > best.peak) {
            best = HeaderColons { count: colons.len(), peak, scale };
        }
        // A full header row pair at confident strength settles the gate; no
        // later scale can change the outcome.
        if best.count >= 2 && best.peak >= TIME_GATE_STRONG_COLON {
            break;
        }
    }
    Ok(best)
}

// Finds a mission number (1-9) by anchoring on ':' in the label region and
// taking the strongest single digit immediately to its left on the same line.
fn find_mission_from_colons(
    label_region: &(impl MatTraitConst + ToInputArray),
    colon_tmpl: &Mat,
    digit_tmpls: &[Mat],
) -> Result<FoundMission> {
    let none = FoundMission { mission: -1, score: -1.0, colon_cx: -1, colon_cy: -1 };
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

    // Anchor only on confident colons. A real header colon clears ~0.9, while
    // the low glyph threshold would also match noise all over a textured
    // background -- each spurious hit then triggers a 10-digit search and an
    // O(n^2) suppression, which is what made the per-scale mission search slow.
    let mut colons = Vec::new();
    collect_detections(label_region, colon_tmpl, COLON_ANCHOR_THRESHOLD, 0, &mut colons)?;
    let colons = suppress(colons, colon_w, colon_h, 0.5);

    let mut best = FoundMission { mission: -1, score: -1.0, colon_cx: -1, colon_cy: -1 };
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
                    best = FoundMission {
                        mission: v as i32,
                        score: d.score,
                        colon_cx: colon.x + colon_w / 2,
                        colon_cy: colon_center_y.round() as i32,
                    };
                }
            }
        }
    }

    Ok(best)
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
    // Widen the colon suppression horizontally (cell_w = 2*colon_w gives a
    // ~colon_w radius) so a side-lobe peak a few pixels from the true colon is
    // merged away; a stray second colon next to the real one would otherwise
    // anchor a bogus reading off the neighbouring glyphs. The vertical radius is
    // left at half a colon height so the Time and Best-Time rows (~one digit
    // height apart) stay distinct.
    let colons = suppress(colons, colon_w * 2, colon_h, 0.5);

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
    // Suppress with a wider neighbourhood (0.7 of a digit cell) than the colon
    // pass uses: two adjacent glyphs blur together into a phantom "8" centred in
    // the gap between them, a few pixels from each real digit. Real digits sit a
    // full digit-width (~1.1 cells) apart, so a 0.7-cell radius drops the lower
    // scoring phantom without ever merging two genuine digits. Without this the
    // phantom is picked as one of the two nearest digits and corrupts the
    // reading (e.g. "00" -> "80", "30" -> "38").
    let digits = suppress(digits, digit_w, digit_h, 0.7);

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
        // A time is an "mm:ss" value capped at 0x3ff (1023) seconds, so its
        // seconds field is always 0-59 and its minutes field never exceeds 17
        // (17:02 = 1022 is the largest in-range value; 18:00 already overflows).
        // A phantom colon landing a few pixels from a real one reads its
        // neighbouring glyphs in the wrong order and yields an impossible field
        // (e.g. "11:71" off the "01:17" row); rejecting out-of-range minutes or
        // seconds drops that bogus reading without touching any genuine time.
        if seconds >= 60 || minutes > 17 {
            continue;
        }
        let total_seconds = minutes * 60 + seconds;
        if total_seconds < 0x3ff {
            times.push(FoundTime { y: colon.y, x: colon.x, seconds: total_seconds });
        }
    }

    dbg_cv!("[times] colons={} times={:?}", colons.len(), times.iter().map(|t| (t.x, t.y, t.seconds)).collect::<Vec<_>>());
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

// The scale at which a frame's overlay was found, remembered so later frames
// can skip the multi-scale search. A capture's resolution (and therefore the
// overlay scale) is fixed for a whole session, so once one overlay frame has
// been resolved the rest can be matched at exactly that scale. Keyed by source
// dimensions so a resolution change transparently forces a fresh search.
#[derive(Clone, Copy)]
struct ScaleCache {
    src_w: i32,
    src_h: i32,
    // Template scale on the downscaled work frame (gate, part/difficulty, times).
    overlay_scale: f64,
    // Template scale on the native frame (mission digit).
    mission_scale: f64,
    // Native-resolution centre of the "Mission N:" colon, so a later frame reads
    // the digit in a tight box around it instead of scanning the header band.
    mission_cx: i32,
    mission_cy: i32,
}

pub struct CvMatcher {
    parts: Vec<Mat>,
    diffs: Vec<Mat>,
    colon: Mat,
    digits: Vec<Mat>,
    // Banner templates that identify the screen. `objectives` ("PRIMARY
    // OBJECTIVES:") marks the level-start briefing; `statistics` ("STATISTICS:")
    // the post-mission stats screen; `special` ("SPECIAL OPTIONS:") the 007
    // options screen; `difficulty` ("DIFFICULTY:") the difficulty-select screen.
    objectives: Mat,
    statistics: Mat,
    special: Mat,
    difficulty: Mat,
    // Status-value templates for the four report screens, which share a
    // "REPORT:" banner and differ only in the status word one line below.
    status_complete: Mat,
    status_failed: Mat,
    status_abort: Mat,
    status_kia: Mat,
    // Scale learned from the first resolved overlay; reused to fast-path every
    // later frame at the same source resolution.
    scale_cache: Mutex<Option<ScaleCache>>,
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

        let objectives = load_template(templates_dir, lang, "objectives")?;
        let statistics = load_template(templates_dir, lang, "statistics")?;
        let special = load_template(templates_dir, lang, "special")?;
        let difficulty = load_template(templates_dir, lang, "difficulty")?;
        let status_complete = load_template(templates_dir, lang, "status_complete")?;
        let status_failed = load_template(templates_dir, lang, "status_failed")?;
        let status_abort = load_template(templates_dir, lang, "status_abort")?;
        let status_kia = load_template(templates_dir, lang, "status_kia")?;

        Ok(CvMatcher {
            parts,
            diffs,
            colon,
            digits,
            objectives,
            statistics,
            special,
            difficulty,
            status_complete,
            status_failed,
            status_abort,
            status_kia,
            scale_cache: Mutex::new(None),
        })
    }

    // Reads the mission number inside `rect` of the native-resolution `gray`,
    // sweeping `scales` and stopping at the first that lands a confident digit
    // (the resolution-implied scale is tried first). Returns the match and the
    // scale it was found at. Coordinates in the result are relative to `rect`.
    fn read_mission(&self, gray: &Mat, rect: Rect, scales: &[f64]) -> Result<(FoundMission, f64)> {
        let region = gray.roi(rect)?;
        let mut found = FoundMission { mission: -1, score: GLYPH_THRESHOLD, colon_cx: -1, colon_cy: -1 };
        let mut scale_used = scales.first().copied().unwrap_or(1.0);
        for &scale in scales {
            let colon_tmpl = scaled(&self.colon, scale)?;
            let mut digit_tmpls = Vec::with_capacity(10);
            for v in 0..=9 {
                digit_tmpls.push(scaled(&self.digits[v], scale)?);
            }
            let f = find_mission_from_colons(&region, &colon_tmpl, &digit_tmpls)?;
            dbg_cv!("[mission] scale={scale:.3} m={} score={:.3} cx={} cy={}", f.mission, f.score, f.colon_cx, f.colon_cy);
            if f.score >= found.score {
                found = f;
                scale_used = scale;
            }
            if found.score >= MISSION_STRONG {
                break;
            }
        }
        Ok((found, scale_used))
    }

    // Identifies the overlay screen by matching each screen's banner word in
    // the band below the header, and the four report screens' status values in
    // the band beneath that, at the scale already established from the header
    // glyphs. The strongest match above the threshold wins; nothing above it
    // leaves the screen `Unknown`. Reading the screen lets the caller skip the
    // time search on every screen but `Stats` (the only one with timed rows).
    //
    // A single scale is enough for the common case because the banner/status
    // words are short and scale-tolerant -- the long "KILLED IN ACTION" status
    // is templated on just its distinctive "ACTION" so it stays as tolerant as
    // the rest. Only when that single pass comes up `Unknown` (an overlay
    // captured a few percent off the header-implied scale) is a small
    // neighbour-scale sweep run to recover it, so the per-frame cost stays at
    // one match per template for nearly every frame.
    fn classify_screen(&self, frame: &Mat, scale: f64) -> Result<Screen> {
        // Sub-region of `frame` given as fractional (x, y, w, h).
        let region = |r: (f64, f64, f64, f64)| -> Result<Mat> {
            let (rx, ry, rw, rh) = r;
            let x0 = (frame.cols() as f64 * rx) as i32;
            let y0 = (frame.rows() as f64 * ry) as i32;
            let w = ((frame.cols() as f64 * rw) as i32).min(frame.cols() - x0).max(1);
            let h = ((frame.rows() as f64 * rh) as i32).min(frame.rows() - y0).max(1);
            frame.roi(Rect::new(x0, y0, w, h))?.try_clone()
        };
        let banner = region(SCREEN_BANNER_REGION)?;
        let status = region(SCREEN_STATUS_REGION)?;

        let candidates: [(Screen, &Mat, &Mat); 8] = [
            (Screen::Start, &self.objectives, &banner),
            (Screen::Stats, &self.statistics, &banner),
            (Screen::Opts007, &self.special, &banner),
            (Screen::Select, &self.difficulty, &banner),
            (Screen::Complete, &self.status_complete, &status),
            (Screen::Failed, &self.status_failed, &status),
            (Screen::Abort, &self.status_abort, &status),
            (Screen::Kia, &self.status_kia, &status),
        ];

        let mut best = Screen::Unknown;
        let mut best_score_v = -1.0;
        let search = |scale: f64, best: &mut Screen, best_score_v: &mut f64| -> Result<()> {
            for (scr, tmpl, region) in candidates {
                let s = best_score(region, &scaled(tmpl, scale)?)?;
                dbg_cv!("[screen] {scr:?} scale={scale:.3} score={s:.3}");
                if s > *best_score_v {
                    *best_score_v = s;
                    *best = scr;
                }
            }
            Ok(())
        };

        search(scale, &mut best, &mut best_score_v)?;
        // Recover an off-scale overlay only when the implied scale resolved
        // nothing; the true screen's word climbs above the bar at its real
        // scale while the others stay well below it.
        if best_score_v < SCREEN_THRESHOLD {
            for m in [0.95, 1.05, 0.90, 1.10] {
                search(scale * m, &mut best, &mut best_score_v)?;
                if best_score_v >= SCREEN_THRESHOLD {
                    break;
                }
            }
        }

        dbg_cv!("[screen] => {best:?} ({best_score_v:.3})");
        Ok(if best_score_v >= SCREEN_THRESHOLD { best } else { Screen::Unknown })
    }

    pub fn match_level_from_raw_bytes(&self, data: *mut u8, w: u32, h: u32) -> Result<LevelMatch> {
        let total_bytes = (w * h * 4) as usize;
        let data_slice = unsafe { std::slice::from_raw_parts(data, total_bytes) };
        self.match_level_from_bgra_bytes(data_slice, w, h)
    }

    /// Matches a `w x h` BGRA frame held in a borrowed byte slice. The slice must
    /// be `w * h * 4` bytes (8-bit BGRA). This is the safe entry point the
    /// monitor uses; `match_level_from_raw_bytes` is the FFI wrapper around it.
    pub fn match_level_from_bgra_bytes(&self, data: &[u8], w: u32, h: u32) -> Result<LevelMatch> {
        let bgra_frame = Mat::new_rows_cols_with_bytes::<core::Vec4b>(h as i32, w as i32, data)?;
        self.match_level_from_bgra_frame(&bgra_frame)
    }

    pub fn match_level_from_bgra_frame(&self, bgra_frame: &impl ToInputArray) -> Result<LevelMatch> {
        let mut result =
            LevelMatch { screen: Screen::Unknown, mission: -1, part: -1, difficulty: -1, times: Vec::new(), runtime_ms: 0.0 };
        let mut timer = PhaseTimer::new();

        // Convert the BGRA frame to grayscale once; every template is matched
        // against this single-channel frame.
        let mut gray = Mat::default();
        imgproc::cvt_color_def(bgra_frame, &mut gray, imgproc::COLOR_BGRA2GRAY)?;

        // Template matching cost grows with frame area, so a native 1080p (or
        // larger) capture is ~5x more expensive than a 480p composite grab for
        // no accuracy gain: the overlay glyphs are large and the templates are
        // softened to tolerate blur anyway. Downscale tall frames to a fixed
        // working height so match time is bounded regardless of source
        // resolution. Only seconds/labels are returned (no pixel coordinates),
        // so the downscale needs no coordinate remapping.
        // `gray` keeps native resolution for the mission-digit read (digits need
        // the detail to tell e.g. 5 from 8); `frame` is the downscaled copy used
        // for the area-heavy gate/label/briefing matches that tolerate blur.
        let frame = if gray.rows() > WORK_HEIGHT {
            let scale = WORK_HEIGHT as f64 / gray.rows() as f64;
            let w = ((gray.cols() as f64 * scale).round() as i32).max(1);
            let mut out = Mat::default();
            imgproc::resize(&gray, &mut out, Size::new(w, WORK_HEIGHT), 0.0, 0.0, imgproc::INTER_AREA)?;
            out
        } else {
            gray.try_clone()?
        };
        timer.lap("grayscale+downscale");

        // Scales to try are derived from the frame height, so each resolution
        // only searches the handful of scales near its own.
        let scales = candidate_scales(frame.rows());

        // If a previous frame at this resolution already resolved the overlay
        // scale, reuse it: the gate and mission searches then try just that one
        // scale instead of sweeping the ladder. The first overlay frame still
        // pays the full search to learn the scale (stored at the end).
        let (src_w, src_h) = (gray.cols(), gray.rows());
        let hint = self
            .scale_cache
            .lock()
            .ok()
            .and_then(|c| *c)
            .filter(|c| c.src_w == src_w && c.src_h == src_h);
        let gate_scales: Vec<f64> = match hint {
            Some(c) => vec![c.overlay_scale],
            None => scales.clone(),
        };

        // Entry gate: the stats overlay (both the level-start briefing and the
        // post-mission stats screen) carries a stack of left-aligned header
        // rows, each ending in a colon ("<Difficulty>:", "Mission N:",
        // "Part <roman>:"). Requiring two strong colons in that header band
        // admits both screens while rejecting busy gameplay frames cheaply, and
        // the scale at which they matched is reused below so the labels are not
        // re-searched across every scale.
        let header = detect_header_colons(
            &frame,
            &self.colon,
            &gate_scales,
            TIME_GATE_COLON_THRESHOLD,
            (HEADER_REGION_X, HEADER_REGION_Y, HEADER_REGION_W, HEADER_REGION_H),
        )?;
        let has_header = header.count >= 2 && header.peak >= TIME_GATE_STRONG_COLON;
        dbg_cv!(
            "[gate] header_colons={} best_colon={:.3} scale={:.3} has_header={has_header} frame={}x{}",
            header.count,
            header.peak,
            header.scale,
            frame.cols(),
            frame.rows()
        );
        timer.lap("header gate");
        if !has_header {
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

        // Read the mission number on the NATIVE-resolution frame: anchor on ':'
        // and take the strongest single digit immediately to its left. The
        // search is confined to a small top-left box (the header rows, excluding
        // the objectives/stats rows below) so it stays cheap even at native res.
        //
        // The scale is swept (cold) because the colon is scale tolerant but the
        // digit is not: at the wrong scale a letter on the "<Difficulty>:" row
        // (e.g. the tail of "Agent:") can out-match the real mission digit. At
        // native resolution the real digit is crisp and tops the early-exit bar
        // at its true scale, so the common case resolves on the first scale.
        let mission_scales: Vec<f64> = match hint {
            Some(c) => vec![c.mission_scale],
            None => candidate_scales(gray.rows()),
        };
        // Search box. Cold: the header band (excludes title and the rows below).
        // Warm: a tight box around the mission colon found on the first overlay
        // frame -- the overlay is fixed for the session, so the digit is read in
        // a few hundred pixels instead of the whole header, the bulk of the
        // per-frame cost at native resolution.
        let header_box = || {
            let x = (gray.cols() as f64 * MISSION_REGION_X) as i32;
            let y = (gray.rows() as f64 * MISSION_REGION_Y) as i32;
            let w = (gray.cols() as f64 * MISSION_REGION_W) as i32;
            let h = (gray.rows() as f64 * MISSION_REGION_H) as i32;
            Rect::new(x, y, w.max(1), h.max(1))
        };
        let mission_rect = match hint {
            Some(c) if c.mission_cx >= 0 => {
                let ch = (self.colon.rows() as f64 * c.mission_scale).round().max(1.0) as i32;
                let x0 = (c.mission_cx - ch * 6).max(0);
                let y0 = (c.mission_cy - ch * 2).max(0);
                let x1 = (c.mission_cx + ch * 2).min(gray.cols());
                let y1 = (c.mission_cy + ch * 2).min(gray.rows());
                Rect::new(x0, y0, (x1 - x0).max(1), (y1 - y0).max(1))
            }
            _ => header_box(),
        };

        let (mut found, mut mission_scale) = self.read_mission(&gray, mission_rect, &mission_scales)?;
        let mut mission_rect = mission_rect;
        // Warm box missed (capture jitter / overlay shifted): retry the full
        // header band at the cached scale before giving up.
        if found.mission < 0 && hint.is_some() {
            mission_rect = header_box();
            let (f, s) = self.read_mission(&gray, mission_rect, &mission_scales)?;
            found = f;
            mission_scale = s;
        }
        result.mission = found.mission;

        // Absolute native colon centre (the search box origin offset added back),
        // used to anchor the label bands and to seed the location cache.
        let mission_cx = if found.colon_cx >= 0 { found.colon_cx + mission_rect.x } else { -1 };
        let mission_cy_native = if found.colon_cy >= 0 { found.colon_cy + mission_rect.y } else { -1 };
        let mission_cy_frame = if mission_cy_native >= 0 {
            (mission_cy_native as f64 * frame.rows() as f64 / gray.rows() as f64).round() as i32
        } else {
            -1
        };
        // Labels run on the downscaled frame at the gate's scale.
        let mut global_scale = header.scale;
        timer.lap("mission");

        // The difficulty row sits one glyph-line above the mission row and the
        // part row one line below, both left-aligned. Anchoring each label
        // search to a short band around the mission row (rather than scanning the
        // whole upper-left corner) cuts the label matching several fold. A band
        // of three colon-heights either side absorbs line-spacing variation.
        let colon_h = (self.colon.rows() as f64 * global_scale).round() as i32;
        let mission_cy = mission_cy_frame;
        let pad = ((colon_h as f64) * 0.4) as i32;

        result.part = if mission_cy >= 0 && colon_h > 0 {
            best_label_in_band(&label_region, &self.parts, global_scale, LABEL_THRESHOLD, mission_cy + pad, mission_cy + colon_h * 3)?
        } else {
            -1
        };
        // Fall back to a full-region scale sweep when the anchored band misses,
        // which also recovers the true scale on off-scale captures.
        if result.part < 0 {
            let (part, part_scale) =
                best_label_over_scales(&label_region, &self.parts, &scales, LABEL_THRESHOLD)?;
            if part >= 0 {
                result.part = part;
                global_scale = part_scale;
                dbg_cv!("[scale recovery] part={part} scale={part_scale:.3}");
            }
        }
        timer.lap("part label");

        let colon_h = (self.colon.rows() as f64 * global_scale).round() as i32;
        let mut difficulty_label = if mission_cy >= 0 && colon_h > 0 {
            best_label_in_band(&label_region, &self.diffs, global_scale, LABEL_THRESHOLD, mission_cy - colon_h * 3, mission_cy - pad)?
        } else {
            -1
        };
        if difficulty_label < 0 {
            difficulty_label = best_label(&label_region, &self.diffs, global_scale, LABEL_THRESHOLD)?;
        }
        result.difficulty = if difficulty_label >= 0 { difficulty_label.saturating_sub(1) } else { -1 };
        timer.lap("difficulty label");

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

        // Identify the overlay screen from its banner / status value. Only the
        // stats screen carries timed rows, so reading the screen lets the time
        // search be skipped on every other screen (start lists objectives, the
        // report screens list per-objective results, etc.), which would
        // otherwise be mis-read as times.
        let screen = self.classify_screen(&frame, global_scale)?;
        result.screen = screen;
        timer.lap("screen classify");

        let times = if screen != Screen::Stats || colon_tmpl.empty() || digit_width_sum == 0 {
            Vec::new()
        } else {
            find_times_band(&frame, &colon_tmpl, &digit_tmpls)?
        };
        result.times = times.into_iter().map(|t| t.seconds).collect();
        timer.lap("time assembly");

        // Learn the scale from this fully-resolved overlay (slow path only) so
        // subsequent frames at the same resolution fast-path the scale search.
        // Require both labels found, so a partial/ambiguous match never poisons
        // the cache with a wrong scale.
        if hint.is_none() && result.mission >= 0 && result.part >= 0 {
            if let Ok(mut cache) = self.scale_cache.lock() {
                *cache = Some(ScaleCache {
                    src_w,
                    src_h,
                    overlay_scale: global_scale,
                    mission_scale,
                    mission_cx,
                    mission_cy: mission_cy_native,
                });
                dbg_cv!("[scale cache] stored overlay={global_scale:.3} mission={mission_scale:.3} colon=({mission_cx},{mission_cy_native}) for {src_w}x{src_h}");
            }
        }

        result.runtime_ms = timer.start().elapsed().as_secs_f64() * 1000.0;

        Ok(result)
    }
}
