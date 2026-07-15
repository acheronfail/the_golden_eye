// Standalone CLI for exercising the GoldenEye level matcher outside of OBS:
// `test_match <lang> path/to/screenshot.png [templates_dir]`. Loads the image as
// BGRA, runs the matcher, and prints the result.

use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use opencv::core::{self, Mat, Rect, Size, ToInputArray};
use opencv::prelude::*;
use opencv::{Result, imgcodecs, imgproc};
use serde::Serialize;

use crate::ge;
use crate::timer::PhaseTimer;

// Cached count of usable cores. OpenCV is built without TBB/OpenMP, so each
// `match_template` pins one core; independent per-scale/per-template matches are
// spread across spare cores by `par_map`. Queried once; fixed for the process.
fn parallelism() -> usize {
    static N: OnceLock<usize> = OnceLock::new();
    *N.get_or_init(|| thread::available_parallelism().map(|p| p.get()).unwrap_or(1))
}

// Maps `f` over `0..n` in index order, splitting the work into contiguous chunks
// on scoped OS threads so independent template matches run concurrently. Falls
// back to serial for tiny `n` or single-core. Order preserved for replay.
fn par_map<T, F>(n: usize, f: F) -> Vec<T>
where
    T: Send,
    F: Fn(usize) -> T + Sync,
{
    let threads = parallelism().min(n);
    if threads <= 1 {
        return (0..n).map(f).collect();
    }
    let chunk = n.div_ceil(threads);
    let f = &f;
    let parts: Vec<Vec<T>> = thread::scope(|s| {
        let handles: Vec<_> = (0..n)
            .step_by(chunk)
            .map(|base| s.spawn(move || (base..(base + chunk).min(n)).map(f).collect::<Vec<T>>()))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    parts.into_iter().flatten().collect()
}

// Set GE_CV_DEBUG to dump intermediate match scores/detections to stderr.
fn dbg_on() -> bool {
    std::env::var_os("GE_CV_DEBUG").is_some()
}
macro_rules! dbg_cv {
    ($($arg:tt)*) => {
        if dbg_on() { eprintln!($($arg)*); }
    };
}

static TEMPLATE_DIR: OnceLock<String> = OnceLock::new();

pub(crate) fn set_template_dir(path: String) {
    let _ = TEMPLATE_DIR.set(path);
}

pub fn template_dir() -> Option<String> {
    TEMPLATE_DIR.get().cloned()
}

// Correlation needed to accept a mission/part/difficulty label match.
const LABEL_THRESHOLD: f64 = 0.70;

// Fraction of the frame searched for the mission/part/difficulty labels. They
// always sit in the upper-left of the stats overlay, so only the top 50% /
// left 60% needs to be searched.
const LABEL_REGION_W: f64 = 0.60;
const LABEL_REGION_H: f64 = 0.50;

// Box searched for the mission digit, as fractions of the frame. Spans the three
// header rows but excludes the title above and the objectives/stats rows below,
// so the anchor never latches an unrelated colon; kept left, near the margin.
const MISSION_REGION_X: f64 = 0.0;
const MISSION_REGION_W: f64 = 0.40;
const MISSION_REGION_Y: f64 = 0.18;
const MISSION_REGION_H: f64 = 0.26;
// Mission-digit correlation that ends the scale sweep early. The implied scale
// is tried first and lands the real digit at ~0.95-0.97; 0.90 settles the common
// case in one pass. Off-scale captures fall through to the remaining scales.
const MISSION_STRONG: f64 = 0.90;

// Region searched for the time colons (upper stats table). Kept generous to
// tolerate overlay drift from letterboxing/rescaling; downstream "mm:ss" spacing
// checks reject label colons. Bottom ~0.62 catches Time/Best but not lower rows.
const COLON_REGION_X: f64 = 0.15;
const COLON_REGION_W: f64 = 0.62;
const COLON_REGION_Y: f64 = 0.45;
const COLON_REGION_H: f64 = 0.17;

// Region searched by the entry gate for stats-overlay header colons. Both the
// level-start and stats screens carry the same three left-aligned header rows
// ending in colons; counting strong colons admits them but rejects gameplay.
const HEADER_REGION_X: f64 = 0.08;
const HEADER_REGION_W: f64 = 0.56;
const HEADER_REGION_Y: f64 = 0.18;
const HEADER_REGION_H: f64 = 0.30;

// Screen classification. Each header screen has a banner word below the header
// stack; the four report screens share a "REPORT:" banner and differ in a status
// value below it. Strongest template match in its band above this threshold wins.
const SCREEN_THRESHOLD: f64 = 0.78;
// (x, y, w, h) as fractions of the frame.
const SCREEN_BANNER_REGION: (f64, f64, f64, f64) = (0.04, 0.39, 0.56, 0.11);
const SCREEN_STATUS_REGION: (f64, f64, f64, f64) = (0.18, 0.47, 0.48, 0.10);
// Language detection uses the side tab on the level-start briefing: short,
// static, and distinct between the English and Japanese ROMs, so it rejects a
// wrong ROM/template language before a same-shaped banner is misclassified.
const LANGUAGE_START_THRESHOLD: f64 = 0.82;
const LANGUAGE_START_MARGIN: f64 = 0.12;
// The language marker is the vertical START tab on the right of the level-start
// briefing. It is fixed near the top-right of both 4:3 and 16:9 captures, so
// there is no need to search the whole frame.
const LANGUAGE_START_REGION: (f64, f64, f64, f64) = (0.68, 0.035, 0.30, 0.35);
// The mission-select grid carries none of the shared header colons, so the gate
// rejects it. It is instead recognized by its film-strip divider (static, en/jp
// identical); strongest match above this threshold classifies it as `Levels`.
const LEVELS_THRESHOLD: f64 = 0.68;
// (x, y, w, h) as fractions of the frame: a band over the left half of the film
// strip spanning the first two inter-row dividers. Two give redundancy (the
// crosshair can cover one); a tight band keeps the match cheap.
const LEVELS_REGION: (f64, f64, f64, f64) = (0.04, 0.20, 0.52, 0.42);

// Correlation needed to accept an individual digit/colon glyph.
const GLYPH_THRESHOLD: f64 = 0.78;
// Colon correlation required to anchor a mission-number search. Higher than the
// glyph threshold: real header colons clear it, but the tiny colon template
// won't match background texture (each false hit is expensive).
const COLON_ANCHOR_THRESHOLD: f64 = 0.86;
// The entry gate admits a frame only with two header colons AND at least one
// confident match. Thresholds sit low (0.8s / 0.85) to admit blurry composite/
// HDMI grabs yet reject gameplay; any non-stats frame that slips in reads no times.
const TIME_GATE_COLON_THRESHOLD: f64 = 0.84;
const TIME_GATE_STRONG_COLON: f64 = 0.85;

// The templates are authored from a capture whose visible frame is this tall.
// The stats overlay scales with the frame, so a source captured at a different
// height needs the templates resized by (frame_height / REFERENCE_HEIGHT).
const REFERENCE_HEIGHT: f64 = 1080.0;

// Frames taller than this are downscaled to it before matching. 480 matches the
// composite/HDMI captures handled accurately, bounding match time. Exposed so
// live capture can downscale up front (GPU), making this internal step a no-op.
pub const WORK_HEIGHT: i32 = 480;

// GoldenEye renders 4:3, but some HDMI converters stretch it to 16:9, so glyphs
// come out too wide for the single-scale matcher. The always-on manilla folder
// (known proportions) calibrates a horizontal squish. See `calibrate_aspect`.
const TARGET_ASPECT: f64 = 4.0 / 3.0;
// The manilla folder's width:height measures ~1.20-1.26 on clean 4:3 captures. A
// folder wider than this signals a horizontally stretched picture; the threshold
// sits between that native band and the ~1.66 a 16:9-stretched folder measures.
const FOLDER_STRETCH_ASPECT: f64 = 1.45;
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
// A column whose mean brightness is below this counts as a (black) pillarbox bar,
// not content. Real bars sit at ~0, below any GoldenEye background texture, so a
// stretched frame's content is trimmed of bars before squishing back to 4:3.
const BAR_BRIGHTNESS: f64 = 24.0;

// Multipliers searched around the resolution-implied scale. Deriving it from the
// frame height keeps the search cheap and avoids wrong-scale false matches. 1.0
// is tried first; the best-fit global scale is reused for every other template.
const SCALE_MULTIPLIERS: [f64; 7] = [1.0, 0.95, 1.05, 0.90, 1.10, 0.85, 1.15];

// Candidate template scales for a frame `frame_height` pixels tall.
fn candidate_scales(frame_height: i32) -> Vec<f64> {
    let base = frame_height as f64 / REFERENCE_HEIGHT;
    SCALE_MULTIPLIERS.iter().map(|m| base * m).collect()
}

// Horizontal extent [left, right] (inclusive) of non-bar content: the first and
// last columns whose mean brightness rises above `bar_brightness`. Dark bars are
// trimmed; a frame with no bars (or all-dark) yields the full width.
fn content_h_extent(gray: &Mat, bar_brightness: f64) -> Result<(i32, i32)> {
    let w = gray.cols();
    if w <= 0 {
        return Ok((0, 0));
    }
    // Collapse the rows to a single row of per-column means.
    let mut col_means = Mat::default();
    core::reduce(gray, &mut col_means, 0, core::REDUCE_AVG, core::CV_64F)?;
    let means = col_means.data_typed::<f64>()?;

    let mut left = 0;
    while left < w && means[left as usize] < bar_brightness {
        left += 1;
    }
    if left >= w {
        return Ok((0, w - 1));
    }
    let mut right = w - 1;
    while right > left && means[right as usize] < bar_brightness {
        right -= 1;
    }
    Ok((left, right))
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
struct FolderDetection {
    aspect: f64,
    rect: Rect,
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

fn detect_folder_aspect(bgra_frame: &impl ToInputArray, w: i32, h: i32) -> Result<Option<FolderDetection>> {
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

// Templates are authored pixel-sharp, but real composite/HDMI sources blur the
// glyphs. Softening templates with a small Gaussian keeps correlation high on
// blurry input and costs almost nothing on sharp input (tiny kernel).
const TEMPLATE_BLUR_KSIZE: i32 = 3;

#[derive(Clone, Copy, Debug)]
struct Detection {
    x: i32,     // left edge in the frame
    y: i32,     // top edge in the frame
    w: i32,     // glyph width at the matched scale
    h: i32,     // glyph height at the matched scale
    score: f64, // correlation score
    value: i32, // digit value 0-9 (unused for the colon)
}

#[derive(Clone, Copy, Debug)]
struct MatchRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    score: f64,
}

impl MatchRect {
    fn offset(self, dx: i32, dy: i32) -> Self {
        MatchRect { x: self.x + dx, y: self.y + dy, ..self }
    }

    fn from_rect(rect: Rect) -> Self {
        MatchRect { x: rect.x, y: rect.y, w: rect.width, h: rect.height, score: 0.0 }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchRegion {
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnnotationRect {
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnnotationSet {
    pub id: String,
    pub label: String,
    pub annotations: Vec<AnnotationRect>,
}

fn template_annotation(region: &MatchRegion) -> AnnotationRect {
    AnnotationRect {
        label: region.label.clone(),
        x: region.x,
        y: region.y,
        w: region.w,
        h: region.h,
        score: Some(region.score),
    }
}

fn annotation_sets(
    match_regions: &[MatchRegion],
    search_regions: Vec<AnnotationRect>,
    folder_region: Option<AnnotationRect>,
    time_digits: Vec<AnnotationRect>,
) -> Vec<AnnotationSet> {
    let mut sets = Vec::new();
    if !match_regions.is_empty() {
        sets.push(AnnotationSet {
            id: "template_matches".to_owned(),
            label: "Template matches".to_owned(),
            annotations: match_regions.iter().map(template_annotation).collect(),
        });
    }
    if !time_digits.is_empty() {
        sets.push(AnnotationSet {
            id: "time_digits".to_owned(),
            label: "Time digits".to_owned(),
            annotations: time_digits,
        });
    }
    if !search_regions.is_empty() {
        sets.push(AnnotationSet {
            id: "search_rois".to_owned(),
            label: "Search ROIs".to_owned(),
            annotations: search_regions,
        });
    }
    if let Some(folder_region) = folder_region {
        sets.push(AnnotationSet {
            id: "folder_dimensions".to_owned(),
            label: "Folder dimensions".to_owned(),
            annotations: vec![folder_region],
        });
    }
    sets
}

// A time recovered from the screen, kept with its position so the final array
// can be ordered top-to-bottom then left-to-right.
struct FoundTime {
    y: i32,
    x: i32,
    seconds: i32,
    colon: MatchRect,
}

fn format_seconds(seconds: i32) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

#[derive(Clone, Copy)]
struct FoundMission {
    mission: i32,
    score: f64,
    // Centre of the anchoring "Mission N:" colon, in region coordinates. The
    // vertical centre pins the difficulty (up) and part (down) rows; both let a
    // later frame re-search the mission in a tight box instead of the header.
    colon_cx: i32,
    colon_cy: i32,
    colon: Option<MatchRect>,
    digit: Option<MatchRect>,
}

// Which overlay screen a frame shows. All but `Levels` share the
// mission/part/difficulty header; they are told apart by the banner word below
// it or, for report screens, the status value. `Unknown` covers gameplay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
    Levels,
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
            Screen::Levels => "levels",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LevelMatch {
    pub screen: Screen,
    pub mission: i32,
    pub part: i32,
    pub difficulty: i32,
    /// ROM language detected from language-specific static UI, when a strong
    /// signal is visible. Currently emitted on level-start briefing screens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_lang: Option<String>,
    /// The stats-screen times split into run / target / best (see [`ge::Times`]).
    /// `None` on any screen that carries no timed rows (start, report, gameplay).
    pub times: Option<ge::Times>,
    /// Raw times read off the overlay top-to-bottom, before classification (the
    /// source `times` derives from). Empty on untimed screens. Kept for the test
    /// harness; production code uses the classified `times` instead.
    pub raw_times: Vec<i32>,
    /// Optional template-match rectangles for developer tooling. These are
    /// empty unless annotation diagnostics are explicitly enabled.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub match_regions: Vec<MatchRegion>,
    /// Developer-only annotation sets. The normal monitor path leaves this
    /// empty so no annotation collection work is done per frame.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub annotation_sets: Vec<AnnotationSet>,
    pub runtime_ms: f64,
}

impl LevelMatch {
    /// Whether this match describes the same on-screen state as `other`,
    /// ignoring `runtime_ms` (the per-frame match cost, which changes every
    /// frame and would otherwise defeat any "only on change" deduplication).
    pub fn same_state(&self, other: &LevelMatch) -> bool {
        self.screen == other.screen
            && self.mission == other.mission
            && self.part == other.part
            && self.difficulty == other.difficulty
            && self.detected_lang == other.detected_lang
            && self.times == other.times
    }
}

fn screen_requires_overlay_markers(screen: Screen) -> bool {
    matches!(screen, Screen::Start | Screen::Stats | Screen::Complete | Screen::Failed | Screen::Abort | Screen::Kia)
}

fn has_overlay_markers(result: &LevelMatch) -> bool {
    result.mission >= 0 && result.part >= 0 && result.difficulty >= 0
}

fn reject_untrusted_screen(result: &mut LevelMatch) {
    let missing_required_markers = screen_requires_overlay_markers(result.screen) && !has_overlay_markers(result);
    let stats_without_times = result.screen == Screen::Stats && result.raw_times.is_empty();
    if missing_required_markers || stats_without_times {
        result.screen = Screen::Unknown;
        result.times = None;
        result.raw_times.clear();
    }
}

// Loads "<dir>/<lang>-<name>.png" as a single-channel (grayscale) template.
// Returns an empty Mat when the file is missing or unreadable.
fn load_template(dir: &str, lang: &str, name: &str) -> Result<Mat> {
    // `dir` may be a canonicalized path. On Windows that is verbatim (`\\?\`),
    // where '/' is a literal char, so `format!("{dir}/...")` would silently miss
    // every template. `Path::join` uses the native separator and stays correct.
    let path = std::path::Path::new(dir).join(format!("{lang}-{name}.png"));
    // Some templates are intentionally absent for a language (e.g. jp has no
    // difficulty-select banner). Skip the read to avoid a spurious OpenCV
    // warning; an empty Mat means "no template" to every caller.
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

// Best single-location match of `tmpl` against `frame`.
fn best_match(frame: &(impl MatTraitConst + ToInputArray), tmpl: &Mat) -> Result<Option<MatchRect>> {
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

// Picks the highest-scoring template from `templates` (matched at `scale`).
// Returns the 1-based index of the winner and its rectangle.
fn best_label_match(
    frame: &(impl MatTraitConst + ToInputArray),
    templates: &[Mat],
    scale: f64,
    threshold: f64,
) -> Result<Option<(i32, MatchRect)>> {
    // Own the (small) region so the per-template closures can share a `&Mat`
    // across the scoped threads, then match every label template in parallel.
    let frame = frame.try_clone()?;
    let frame = &frame;
    let scores: Vec<Result<Option<MatchRect>>> =
        par_map(templates.len(), |i| best_match(frame, &scaled(&templates[i], scale)?));

    let mut best = -1;
    let mut best_rect = None;
    let mut best_score_v = threshold;
    for (i, r) in scores.into_iter().enumerate() {
        let Some(r) = r? else { continue };
        let s = r.score;
        dbg_cv!("[label] idx={} scale={scale:.3} score={s:.3}", i + 1);
        if s >= best_score_v {
            best_score_v = s;
            best = i as i32 + 1;
            best_rect = Some(r);
        }
    }
    Ok(best_rect.map(|r| (best, r)))
}

// Best label within a horizontal band of `region` spanning rows [y0, y1).
// Coordinates in the returned rectangle are relative to `region`.
fn best_label_in_band_match(
    region: &(impl MatTraitConst + ToInputArray),
    templates: &[Mat],
    scale: f64,
    threshold: f64,
    y0: i32,
    y1: i32,
) -> Result<Option<(i32, MatchRect)>> {
    let y0 = y0.clamp(0, region.rows());
    let y1 = y1.clamp(0, region.rows());
    if y1 - y0 < 2 {
        return Ok(None);
    }
    let band = region.roi(Rect::new(0, y0, region.cols(), y1 - y0))?;
    Ok(best_label_match(&band, templates, scale, threshold)?.map(|(idx, r)| (idx, r.offset(0, y0))))
}

// Like `best_label_match`, but also sweeps `scales`. Used to recover the true
// overlay scale when the scale implied by the frame height is wrong.
fn best_label_match_over_scales(
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
                out.push(Detection { x, y, w, h: tmpl.rows(), score: score as f64, value });
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

// Detects header colons inside `region` (fractional x/y/w/h), trying every
// candidate scale so the gate works at any capture resolution. Returns the
// richest result (most colons, then peak) and its scale, stopping early.
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
    // Materialize the ROI into an owned Mat once (the region is small) so the
    // parallel scale closures can share a plain `&Mat` -- the BoxedRef a ROI
    // yields is not Deref/Sync-shareable across the scoped threads.
    let colon_region = colon_region.try_clone()?;
    let colon_region = &colon_region;

    // Score every scale in parallel: each one resizes the colon template and
    // counts suppressed colon hits in the region. The scales are independent,
    // so this is the per-scale match work spread across cores.
    let scored: Vec<Result<Option<(usize, f64)>>> = par_map(scales.len(), |i| {
        let scale = scales[i];
        let colon_tmpl = scaled(base_colon, scale)?;
        if colon_tmpl.empty() || colon_tmpl.rows() > colon_region.rows() || colon_tmpl.cols() > colon_region.cols() {
            return Ok(None);
        }
        let mut colons = Vec::new();
        collect_detections(colon_region, &colon_tmpl, threshold, 0, &mut colons)?;
        let colons = suppress(colons, colon_tmpl.cols(), colon_tmpl.rows(), 0.5);
        let peak = colons.iter().map(|d| d.score).fold(-1.0, f64::max);
        Ok(Some((colons.len(), peak)))
    });

    // Replay the sequential selection over the parallel results so the chosen
    // scale matches the serial version: most colons wins, ties break on peak,
    // and stop at the first scale landing a confident header row pair.
    for (i, r) in scored.into_iter().enumerate() {
        let Some((count, peak)) = r? else { continue };
        if count > best.count || (count == best.count && peak > best.peak) {
            best = HeaderColons { count, peak, scale: scales[i] };
        }
        if best.count >= 2 && best.peak >= TIME_GATE_STRONG_COLON {
            break;
        }
    }
    Ok(best)
}

fn header_colon_regions(frame: &Mat, base_colon: &Mat, scale: f64, threshold: f64) -> Result<Vec<MatchRect>> {
    if base_colon.empty() {
        return Ok(Vec::new());
    }
    let colon_tmpl = scaled(base_colon, scale)?;
    if colon_tmpl.empty() {
        return Ok(Vec::new());
    }
    let (rx, ry, rw, rh) = (HEADER_REGION_X, HEADER_REGION_Y, HEADER_REGION_W, HEADER_REGION_H);
    let x0 = (frame.cols() as f64 * rx) as i32;
    let y0 = (frame.rows() as f64 * ry) as i32;
    let region = frame.roi(Rect::new(x0, y0, (frame.cols() as f64 * rw) as i32, (frame.rows() as f64 * rh) as i32))?;
    let mut detections = Vec::new();
    collect_detections(&region, &colon_tmpl, threshold, 0, &mut detections)?;
    Ok(suppress(detections, colon_tmpl.cols(), colon_tmpl.rows(), 0.5)
        .into_iter()
        .map(|d| MatchRect { x: d.x + x0, y: d.y + y0, w: d.w, h: d.h, score: d.score })
        .collect())
}

// Finds a mission number (1-9) by anchoring on ':' in the label region and
// taking the strongest single digit immediately to its left on the same line.
fn find_mission_from_colons(
    label_region: &(impl MatTraitConst + ToInputArray),
    colon_tmpl: &Mat,
    digit_tmpls: &[Mat],
) -> Result<FoundMission> {
    let none = FoundMission { mission: -1, score: -1.0, colon_cx: -1, colon_cy: -1, colon: None, digit: None };
    if colon_tmpl.empty() || digit_tmpls.len() < 10 {
        return Ok(none);
    }

    let mut digit_width_sum = 0;
    for tmpl in &digit_tmpls[1..=9] {
        if tmpl.empty() {
            return Ok(none);
        }
        digit_width_sum += tmpl.cols();
    }
    let digit_w = (digit_width_sum / 9).max(1);
    let digit_h = digit_tmpls[1].rows();
    let colon_w = colon_tmpl.cols();
    let colon_h = colon_tmpl.rows();

    // Own the region so the parallel per-digit closures can share a `&Mat`
    // (a ROI yields a BoxedRef that the scoped threads cannot share). This is
    // the native-resolution mission box, so it is small and cloned once.
    let label_region = label_region.try_clone()?;
    let label_region = &label_region;

    // Anchor only on confident colons. A real header colon clears ~0.9, while the
    // low glyph threshold would match noise on textured background -- each
    // spurious hit triggers a 10-digit search and O(n^2) suppression.
    let mut colons = Vec::new();
    collect_detections(label_region, colon_tmpl, COLON_ANCHOR_THRESHOLD, 0, &mut colons)?;
    let colons = suppress(colons, colon_w, colon_h, 0.5);

    let band_pad_x = digit_w * 2;
    let band_pad_y = digit_h;

    // Each (colon, digit) pair is an independent template search -- the bulk of
    // the mission cost at native resolution. Fan the pairs across cores; each
    // returns its best candidate, reduced to the serial "highest digit wins".
    let work: Vec<(usize, usize)> = (0..colons.len()).flat_map(|c| (1..=9).map(move |v| (c, v))).collect();
    let search_digit = |k: usize| {
        let (ci, v) = work[k];
        let colon = colons[ci];
        let x0 = (colon.x - band_pad_x).max(0);
        let y0 = (colon.y - band_pad_y).max(0);
        let x1 = (colon.x + (colon_w / 2).max(1)).min(label_region.cols());
        let y1 = (colon.y + colon_h + band_pad_y).min(label_region.rows());
        if x1 <= x0 || y1 <= y0 {
            return Ok(None);
        }
        let roi = label_region.roi(Rect::new(x0, y0, x1 - x0, y1 - y0))?;
        let colon_center_y = colon.y as f64 + colon_h as f64 / 2.0;
        let mut per_value = Vec::new();
        collect_detections(&roi, &digit_tmpls[v], GLYPH_THRESHOLD, v as i32, &mut per_value)?;
        let mut best: Option<FoundMission> = None;
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
            if best.is_none_or(|b| d.score >= b.score) {
                best = Some(FoundMission {
                    mission: v as i32,
                    score: d.score,
                    colon_cx: colon.x + colon_w / 2,
                    colon_cy: colon_center_y.round() as i32,
                    colon: Some(MatchRect { x: colon.x, y: colon.y, w: colon.w, h: colon.h, score: colon.score }),
                    digit: Some(MatchRect { x: d.x, y: d.y, w: d.w, h: d.h, score: d.score }),
                });
            }
        }
        Ok(best)
    };
    // Once the mission location is cached this region is only a few glyphs wide,
    // where per-digit threads cost more than the tiny searches. Keep that warm
    // path serial; parallelize only the larger cold header scan.
    let partials: Vec<Result<Option<FoundMission>>> = if label_region.total() < 10_000 {
        (0..work.len()).map(search_digit).collect()
    } else {
        par_map(work.len(), search_digit)
    };

    let mut best = FoundMission { mission: -1, score: -1.0, colon_cx: -1, colon_cy: -1, colon: None, digit: None };
    for p in partials {
        if let Some(cand) = p?
            && cand.score >= best.score
        {
            best = cand;
        }
    }

    Ok(best)
}

fn find_times_band(
    frame: &Mat,
    glyphs: &ScaledGlyphs,
    // When Some, per-digit diagnostic boxes (label, work-coord rect) are collected
    // for the developer overlay: each digit's own detection plus, for the two outer
    // digits, the colon-anchored slot, so a detection/anchor divergence is visible.
    mut diag: Option<&mut Vec<(String, MatchRect)>>,
) -> Result<Vec<FoundTime>> {
    let colon_tmpl = &glyphs.colon;
    let digit_tmpls = &glyphs.digits;
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
    // Widen the colon suppression horizontally (~colon_w radius) so a side-lobe
    // peak near the true colon is merged, avoiding a bogus reading. Vertical
    // radius stays half a colon height so Time and Best-Time rows stay distinct.
    let colons = suppress(colons, colon_w * 2, colon_h, 0.5);

    let band_pad_x = digit_w * 3;
    let band_pad_y = digit_h;
    // Each colon anchors an independent, tiny digit search. Stats screens yield
    // many colon peaks (~20 on blurry fixtures), so ten templates per anchor
    // serially dominated the matcher. Spread anchors across cores, then combine.
    let digit_buckets: Vec<Result<Vec<Detection>>> = par_map(colons.len(), |i| {
        let colon = colons[i];
        let x0 = (colon.x - band_pad_x).max(0);
        let y0 = (colon.y - band_pad_y).max(0);
        let x1 = (colon.x + colon_w + band_pad_x).min(frame.cols());
        let y1 = (colon.y + colon_h + band_pad_y).min(frame.rows());
        if x1 <= x0 || y1 <= y0 {
            return Ok(Vec::new());
        }
        let roi = frame.roi(Rect::new(x0, y0, x1 - x0, y1 - y0))?;
        let mut digits = Vec::new();
        for (v, tmpl) in digit_tmpls.iter().enumerate().take(10) {
            let start = digits.len();
            collect_detections(&roi, tmpl, GLYPH_THRESHOLD, v as i32, &mut digits)?;
            for d in &mut digits[start..] {
                d.x += x0;
                d.y += y0;
            }
        }
        Ok(digits)
    });
    let mut digits = Vec::new();
    for bucket in digit_buckets {
        digits.extend(bucket?);
    }
    // Suppress with a wider neighbourhood (0.7 digit cell) than the colon pass:
    // two adjacent glyphs blur into a phantom "8" between them. Real digits sit
    // ~1.1 cells apart, so 0.7 drops the phantom without merging genuine digits.
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
        right.sort_by_key(|a| a.x);
        left.sort_by_key(|b| std::cmp::Reverse(b.x));

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

        // Per-frame detection positions are accurate, so read each digit there.
        // But the two OUTER digits (tens-min `l1`, units-sec `r1`) are the ones a
        // between-digit phantom peak shadows -- so for them also read the colon-
        // anchored fixed slot and keep whichever the discriminator is surer of. The
        // detection wins when it is well-aligned (per-frame accurate); the anchor
        // only wins when detection landed on a low-confidence phantom.
        let colon_cx = colon.x as f64 + colon_w as f64 / 2.0;
        let digit_y = colon.y + (colon_h - digit_h) / 2;
        let anchor_x = |offset: f64| (colon_cx + offset * digit_w as f64 - digit_w as f64 / 2.0).round() as i32;

        // Read every digit at its own (per-frame accurate) detection.
        let (l1_det, l1_dc) = classify_box(frame, glyphs, Rect::new(l1.x, l1.y, l1.w, l1.h), l1.value)?;
        let (l0_det, l0_dc) = classify_box(frame, glyphs, Rect::new(l0.x, l0.y, l0.w, l0.h), l0.value)?;
        let (r0_det, r0_dc) = classify_box(frame, glyphs, Rect::new(r0.x, r0.y, r0.w, r0.h), r0.value)?;
        let (r1_det, r1_dc) = classify_box(frame, glyphs, Rect::new(r1.x, r1.y, r1.w, r1.h), r1.value)?;
        // For the phantom-prone outer digits, also read the colon-anchored slot and
        // let it win only when clearly more confident than the detection.
        let (l1_ax, r1_ax) = (anchor_x(SLOT_OFFSETS[0]), anchor_x(SLOT_OFFSETS[3]));
        let (l1_anc, l1_ac) = classify_box(frame, glyphs, Rect::new(l1_ax, digit_y, digit_w, digit_h), l1_det)?;
        let (r1_anc, r1_ac) = classify_box(frame, glyphs, Rect::new(r1_ax, digit_y, digit_w, digit_h), r1_det)?;
        let l1v = if l1_ac >= ANCHOR_ACCEPT && l1_ac > l1_dc { l1_anc } else { l1_det };
        let r1v = if r1_ac >= ANCHOR_ACCEPT && r1_ac > r1_dc { r1_anc } else { r1_det };
        let minutes = l1v * 10 + l0_det;
        let seconds = r0_det * 10 + r1v;

        if let Some(diag) = diag.as_deref_mut() {
            let rect = |x, y, w, h, score| MatchRect { x, y, w, h, score };
            diag.push((format!("min-tens det {l1_det} ({l1_dc:.2})"), rect(l1.x, l1.y, l1.w, l1.h, l1_dc)));
            diag.push((format!("min-tens slot {l1_anc} ({l1_ac:.2})"), rect(l1_ax, digit_y, digit_w, digit_h, l1_ac)));
            diag.push((format!("min-units det {l0_det} ({l0_dc:.2})"), rect(l0.x, l0.y, l0.w, l0.h, l0_dc)));
            diag.push((format!("sec-tens det {r0_det} ({r0_dc:.2})"), rect(r0.x, r0.y, r0.w, r0.h, r0_dc)));
            diag.push((format!("sec-units det {r1_det} ({r1_dc:.2})"), rect(r1.x, r1.y, r1.w, r1.h, r1_dc)));
            diag.push((format!("sec-units slot {r1_anc} ({r1_ac:.2})"), rect(r1_ax, digit_y, digit_w, digit_h, r1_ac)));
        }
        // A time is "mm:ss" capped at 0x3ff (1023) s, so seconds are 0-59 and
        // minutes <= 17. A phantom colon reads glyphs out of order into an
        // impossible field; rejecting out-of-range values drops the bogus reading.
        if seconds >= 60 || minutes > 17 {
            continue;
        }
        let total_seconds = minutes * 60 + seconds;
        if total_seconds < 0x3ff {
            times.push(FoundTime {
                y: colon.y,
                x: colon.x,
                seconds: total_seconds,
                colon: MatchRect { x: colon.x, y: colon.y, w: colon.w, h: colon.h, score: colon.score },
            });
        }
    }

    dbg_cv!(
        "[times] colons={} times={:?}",
        colons.len(),
        times.iter().map(|t| (t.x, t.y, t.seconds)).collect::<Vec<_>>()
    );
    let line_bucket = digit_h as f64 * 0.5;
    times.sort_by(|a, b| {
        let ra = (a.y as f64 / line_bucket).round() as i32;
        let rb = (b.y as f64 / line_bucket).round() as i32;
        if ra != rb { ra.cmp(&rb) } else { a.x.cmp(&b.x) }
    });

    // A time-colon can register twice when a side-lobe peak survives suppression,
    // yielding a duplicate time. Collapse times whose colons sit within a glyph;
    // genuine same-row times are many digit-widths apart and preserved.
    let dedup_dy = (digit_h as f64 * 0.3) as i32;
    let mut deduped: Vec<FoundTime> = Vec::with_capacity(times.len());
    for t in times {
        let dup = deduped.iter().any(|k| (t.x - k.x).abs() < digit_w * 2 && (t.y - k.y).abs() < dedup_dy);
        if !dup {
            deduped.push(t);
        }
    }

    Ok(deduped)
}

// Matches the GoldenEye level-stats overlay in a single BGRA frame against the
// template PNGs in `templates_dir`. Mirrors ge_cv_match_level().
pub fn match_level(bgra_frame: &impl ToInputArray, lang: &str, templates_dir: &str) -> Result<LevelMatch> {
    CvMatcher::new(lang, templates_dir)?.match_level_from_bgra_frame(bgra_frame)
}

// The scale a frame's overlay was found at, remembered so later frames skip the
// multi-scale search (resolution is fixed for a session). Keyed by source
// dimensions so a resolution change forces a fresh search.
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

// Colon and digit templates resized for one exact scale, used every frame by the
// mission and time readers. Caching avoids re-resizing/blurring all eleven; Arc
// lets the hot path borrow a set without holding the cache lock during matching.
struct ScaledGlyphs {
    colon: Mat,
    digits: Vec<Mat>,
    // Discriminative digit reader (see `DigitDiscriminator`), built from `digits`.
    // None when a template is missing so callers fall back to plain matching.
    discriminator: Option<DigitDiscriminator>,
}

// Re-classifies an already-located digit by weighting the correlation towards the
// pixels where the ten glyphs actually differ (an `8`'s middle bar, a `6`/`9`
// opening). Whole-glyph correlation drowns those few pixels in the shared outer
// ring, leaving `0/6/8/9` a hair apart; weighting by inter-glyph variance widens
// that margin ~3x, so per-frame capture noise no longer flips the winner.
struct DigitDiscriminator {
    box_w: i32,
    box_h: i32,
    // Each glyph resized to the common box, then mean-centred (see `mean_center`).
    templates: Vec<Vec<f32>>,
    // Per-pixel variance across the ten glyphs: the discriminating weight.
    weights: Vec<f32>,
    weight_sum: f32,
}

impl DigitDiscriminator {
    // Builds the common-box templates and variance weights from scaled glyphs.
    fn build(digits: &[Mat]) -> Result<Option<Self>> {
        if digits.len() < 10 || digits.iter().take(10).any(|d| d.empty()) {
            return Ok(None);
        }
        let box_w = digits.iter().take(10).map(|d| d.cols()).max().unwrap_or(0);
        let box_h = digits.iter().take(10).map(|d| d.rows()).max().unwrap_or(0);
        if box_w < 2 || box_h < 2 {
            return Ok(None);
        }
        let n = (box_w * box_h) as usize;
        let mut templates = Vec::with_capacity(10);
        for d in digits.iter().take(10) {
            templates.push(resize_to_box(d, box_w, box_h)?);
        }
        let mut weights = vec![0f32; n];
        for i in 0..n {
            let mean = templates.iter().map(|t| t[i]).sum::<f32>() / 10.0;
            weights[i] = templates.iter().map(|t| (t[i] - mean).powi(2)).sum::<f32>() / 10.0;
        }
        let weight_sum: f32 = weights.iter().sum();
        if weight_sum <= f32::EPSILON {
            return Ok(None);
        }
        for t in &mut templates {
            mean_center(t);
        }
        Ok(Some(Self { box_w, box_h, templates, weights, weight_sum }))
    }

    // Scores the patch at `rect` against every glyph, returning the best (value,
    // score). The score doubles as a confidence: a well-aligned real digit scores
    // ~0.9, while a patch straddling two glyphs (a phantom between-digit peak)
    // scores far lower, which lets callers reject phantoms.
    fn classify(&self, frame: &Mat, rect: Rect) -> Result<(i32, f64)> {
        let Some(rect) = clamp_rect(rect, frame.cols(), frame.rows()) else { return Ok((-1, -1.0)) };
        let mut patch = resize_to_box(&frame.roi(rect)?, self.box_w, self.box_h)?;
        mean_center(&mut patch);
        let mut best = (-2.0f64, -1i32);
        for (v, tmpl) in self.templates.iter().enumerate() {
            let score = weighted_ncc(&patch, tmpl, &self.weights, self.weight_sum);
            if score > best.0 {
                best = (score, v as i32);
            }
        }
        Ok((best.1, best.0))
    }
}

// Digit-slot centres relative to the colon centre, in digit-widths, for the fixed
// "mm:ss" stats layout: [tens-min, units-min, tens-sec, units-sec]. Measured to be
// consistent across capture sources and scales, so the colon (a stable, distinctive
// anchor) positions the digits far more reliably than per-digit detection.
const SLOT_OFFSETS: [f64; 4] = [-2.0, -0.85, 0.80, 2.0];

// Confidence the colon-anchored slot read must clear to override an outer digit's
// own detection: high enough that only a well-aligned real glyph wins (a phantom
// or a blurry off-slot read stays below it), so it corrects a shadowed digit
// without overriding an accurate per-frame detection.
const ANCHOR_ACCEPT: f64 = 0.90;

// Classifies the digit in `rect` with the discriminator, returning (value,
// confidence). Falls back to `fallback` (the plain detection value) when there is
// no discriminator or the patch is off-frame.
fn classify_box(frame: &Mat, glyphs: &ScaledGlyphs, rect: Rect, fallback: i32) -> Result<(i32, f64)> {
    match glyphs.discriminator.as_ref() {
        Some(disc) => {
            let (value, conf) = disc.classify(frame, rect)?;
            Ok(if value >= 0 { (value, conf) } else { (fallback, -1.0) })
        }
        None => Ok((fallback, 1.0)),
    }
}

// Resizes a single-channel glyph to `box_w x box_h` and returns its pixels as f32.
fn resize_to_box(src: &(impl MatTraitConst + ToInputArray), box_w: i32, box_h: i32) -> Result<Vec<f32>> {
    let mut resized = Mat::default();
    let interp = if src.cols() > box_w { imgproc::INTER_AREA } else { imgproc::INTER_LINEAR };
    imgproc::resize(src, &mut resized, Size::new(box_w, box_h), 0.0, 0.0, interp)?;
    let mut f = Mat::default();
    resized.convert_to(&mut f, core::CV_32F, 1.0, 0.0)?;
    Ok(f.data_typed::<f32>()?.to_vec())
}

// Subtracts the mean in place so `weighted_ncc` compares shape, not brightness.
fn mean_center(v: &mut [f32]) {
    let mean = v.iter().sum::<f32>() / v.len() as f32;
    for x in v.iter_mut() {
        *x -= mean;
    }
}

// Variance-weighted normalised cross-correlation of two mean-centred vectors.
fn weighted_ncc(a: &[f32], b: &[f32], w: &[f32], wsum: f32) -> f64 {
    let wa: f32 = a.iter().zip(w).map(|(x, wi)| x * wi).sum::<f32>() / wsum;
    let wb: f32 = b.iter().zip(w).map(|(x, wi)| x * wi).sum::<f32>() / wsum;
    let mut num = 0f64;
    let mut da = 0f64;
    let mut db = 0f64;
    for ((&ai, &bi), &wi) in a.iter().zip(b).zip(w) {
        let (ca, cb) = ((ai - wa) as f64, (bi - wb) as f64);
        num += wi as f64 * ca * cb;
        da += wi as f64 * ca * ca;
        db += wi as f64 * cb * cb;
    }
    let den = (da * db).sqrt();
    if den > 0.0 { num / den } else { -1.0 }
}

// Clamps `rect` to the frame, returning None if it falls entirely outside.
fn clamp_rect(rect: Rect, cols: i32, rows: i32) -> Option<Rect> {
    let x = rect.x.clamp(0, cols);
    let y = rect.y.clamp(0, rows);
    let w = (rect.x + rect.width).min(cols) - x;
    let h = (rect.y + rect.height).min(rows) - y;
    if w >= 2 && h >= 2 { Some(Rect::new(x, y, w, h)) } else { None }
}

// Aspect correction learned for a source resolution: the horizontal window
// holding the 4:3 picture and the width it resizes to (height untouched).
// Learned once from a folder frame and reused for every frame at that resolution.
#[derive(Clone, Copy)]
struct AspectCalibration {
    // Source dimensions this calibration was measured for.
    src_w: i32,
    src_h: i32,
    // Horizontal content window to keep (drops any dark pillarbox bars).
    crop_x: i32,
    crop_w: i32,
    // Width to resize the kept window to; the height is left unchanged.
    target_w: i32,
    // Source-frame rectangle of the manilla folder measured during calibration.
    folder_rect: Option<Rect>,
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

impl AspectCalibration {
    // A calibration that leaves the frame untouched (already 4:3 / pillarboxed).
    fn identity(src_w: i32, src_h: i32) -> Self {
        AspectCalibration { src_w, src_h, crop_x: 0, crop_w: src_w, target_w: src_w, folder_rect: None }
    }

    // As a source-relative capture transform. Horizontal crop only (full height
    // kept). The crop is a fraction of frame width, equal to the same fraction of
    // source width (the downscale preserves horizontal aspect).
    fn capture_region(&self) -> CaptureRegion {
        CaptureRegion {
            crop_x: self.crop_x as f32 / self.src_w as f32,
            crop_y: 0.0,
            crop_w: self.crop_w as f32 / self.src_w as f32,
            crop_h: 1.0,
            out_aspect: self.target_w as f32 / self.src_h as f32,
        }
    }

    fn is_identity(&self) -> bool {
        self.crop_x == 0 && self.crop_w == self.src_w && self.target_w == self.src_w
    }

    // Applies the correction to a grayscale frame.
    fn apply(&self, gray: &Mat) -> Result<Mat> {
        if self.is_identity() {
            return gray.try_clone();
        }
        let window = gray.roi(Rect::new(self.crop_x, 0, self.crop_w, gray.rows()))?;
        let mut out = Mat::default();
        imgproc::resize(
            &window,
            &mut out,
            Size::new(self.target_w.max(1), gray.rows()),
            0.0,
            0.0,
            imgproc::INTER_AREA,
        )?;
        Ok(out)
    }
}

struct RegionMapper {
    calib: AspectCalibration,
    corrected_w: i32,
    corrected_h: i32,
    work_w: i32,
    work_h: i32,
}

impl RegionMapper {
    fn from_frames(calib: AspectCalibration, corrected: &Mat, work: &Mat) -> Self {
        RegionMapper {
            calib,
            corrected_w: corrected.cols(),
            corrected_h: corrected.rows(),
            work_w: work.cols(),
            work_h: work.rows(),
        }
    }

    fn corrected_to_source(&self, r: MatchRect) -> MatchRegion {
        let x = self.calib.crop_x as f64 + r.x as f64 * self.calib.crop_w as f64 / self.calib.target_w as f64;
        let w = r.w as f64 * self.calib.crop_w as f64 / self.calib.target_w as f64;
        MatchRegion {
            label: String::new(),
            x: x.round() as i32,
            y: r.y,
            w: w.round().max(1.0) as i32,
            h: r.h.max(1),
            score: r.score,
        }
    }

    fn work_to_source(&self, r: MatchRect) -> MatchRegion {
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

fn fractional_rect(cols: i32, rows: i32, region: (f64, f64, f64, f64)) -> Rect {
    let (rx, ry, rw, rh) = region;
    let x0 = (cols as f64 * rx) as i32;
    let y0 = (rows as f64 * ry) as i32;
    let w = ((cols as f64 * rw) as i32).min(cols - x0).max(1);
    let h = ((rows as f64 * rh) as i32).min(rows - y0).max(1);
    Rect::new(x0, y0, w, h)
}

pub struct CvMatcher {
    lang: String,
    diagnostics: bool,
    parts: Vec<Mat>,
    diffs: Vec<Mat>,
    colon: Mat,
    digits: Vec<Mat>,
    // Banner templates that identify the screen: `objectives` (level-start),
    // `statistics` (post-mission stats), `special` (007 options), `difficulty`
    // (difficulty-select).
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
    language_start_en: Mat,
    language_start_jp: Mat,
    // Film-strip divider of the mission-select grid, used to recognize the
    // `Levels` screen (which carries no header colons for the gate to latch on).
    levels: Mat,
    // Scale learned from the first resolved overlay; reused to fast-path every
    // later frame at the same source resolution.
    scale_cache: Mutex<Option<ScaleCache>>,
    // Aspect correction learned from the first frame that shows a manilla
    // folder; reused for every later frame at the same source resolution.
    aspect_cache: Mutex<Option<AspectCalibration>>,
    // Lazily populated because cold scale recovery may try several scales, but
    // a live source normally settles on one work scale and one native scale.
    glyph_cache: Mutex<Vec<(u64, Arc<ScaledGlyphs>)>>,
}

impl CvMatcher {
    pub fn new(lang: &str, templates_dir: &str) -> Result<Self> {
        // Pin OpenCV's parallel backend to one thread: we drive parallelism with
        // `par_map`, so a multi-threaded backend would oversubscribe cores and
        // spike tail latency. `GE_CV_THREADS` opts out for benchmarking.
        if std::env::var_os("GE_CV_THREADS").is_none() {
            let _ = core::set_num_threads(1);
        }

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
        let language_start_en = load_template(templates_dir, "en", "start")?;
        let language_start_jp = load_template(templates_dir, "jp", "start")?;
        let levels = load_template(templates_dir, lang, "levels")?;

        Ok(CvMatcher {
            lang: lang.to_owned(),
            diagnostics: false,
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
            language_start_en,
            language_start_jp,
            levels,
            scale_cache: Mutex::new(None),
            aspect_cache: Mutex::new(None),
            glyph_cache: Mutex::new(Vec::new()),
        })
    }

    pub fn diagnostics_available() -> bool {
        true
    }

    pub fn with_diagnostics(mut self, enabled: bool) -> Self {
        self.diagnostics = enabled;
        self
    }

    pub fn set_diagnostics(&mut self, enabled: bool) {
        self.diagnostics = enabled;
    }

    pub fn diagnostics_enabled(&self) -> bool {
        self.diagnostics
    }

    fn scaled_glyphs(&self, scale: f64) -> Result<Arc<ScaledGlyphs>> {
        let key = scale.to_bits();
        let mut cache = self.glyph_cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((_, glyphs)) = cache.iter().find(|(cached_key, _)| *cached_key == key) {
            return Ok(Arc::clone(glyphs));
        }

        let colon = scaled(&self.colon, scale)?;
        let mut digits = Vec::with_capacity(10);
        for digit in &self.digits {
            digits.push(scaled(digit, scale)?);
        }
        let discriminator = DigitDiscriminator::build(&digits)?;
        let glyphs = Arc::new(ScaledGlyphs { colon, digits, discriminator });
        cache.push((key, Arc::clone(&glyphs)));
        Ok(glyphs)
    }

    fn push_work_region(
        &self,
        out: &mut Vec<MatchRegion>,
        mapper: &RegionMapper,
        label: impl Into<String>,
        rect: MatchRect,
    ) {
        if !self.diagnostics {
            return;
        }
        let mut region = mapper.work_to_source(rect);
        region.label = label.into();
        out.push(region);
    }

    fn push_work_search_region(
        &self,
        out: &mut Vec<AnnotationRect>,
        mapper: &RegionMapper,
        label: impl Into<String>,
        rect: Rect,
    ) {
        if !self.diagnostics {
            return;
        }
        let region = mapper.work_to_source(MatchRect::from_rect(rect));
        out.push(AnnotationRect {
            label: label.into(),
            x: region.x,
            y: region.y,
            w: region.w,
            h: region.h,
            score: None,
        });
    }

    fn push_corrected_region(
        &self,
        out: &mut Vec<MatchRegion>,
        mapper: &RegionMapper,
        label: impl Into<String>,
        rect: MatchRect,
    ) {
        if !self.diagnostics {
            return;
        }
        let mut region = mapper.corrected_to_source(rect);
        region.label = label.into();
        out.push(region);
    }

    fn push_corrected_search_region(
        &self,
        out: &mut Vec<AnnotationRect>,
        mapper: &RegionMapper,
        label: impl Into<String>,
        rect: Rect,
    ) {
        if !self.diagnostics {
            return;
        }
        let region = mapper.corrected_to_source(MatchRect::from_rect(rect));
        out.push(AnnotationRect {
            label: label.into(),
            x: region.x,
            y: region.y,
            w: region.w,
            h: region.h,
            score: None,
        });
    }

    fn folder_annotation(&self, calib: AspectCalibration) -> Option<AnnotationRect> {
        if !self.diagnostics {
            return None;
        }
        let rect = calib.folder_rect?;
        Some(AnnotationRect {
            label: "detected manilla folder".to_owned(),
            x: rect.x,
            y: rect.y,
            w: rect.width,
            h: rect.height,
            score: None,
        })
    }

    // Returns `gray` corrected to 4:3 when the source is a stretched 4:3 picture,
    // else unchanged. Learned once per resolution off the manilla folder and
    // cached; folderless frames inherit an earlier menu frame's calibration.
    fn calibrate_aspect(&self, bgra_frame: &impl ToInputArray, gray: &Mat) -> Result<(Mat, AspectCalibration)> {
        let (w, h) = (gray.cols(), gray.rows());

        // Reuse the calibration already learned for this resolution.
        if let Some(c) = self.aspect_cache.lock().ok().and_then(|c| *c).filter(|c| c.src_w == w && c.src_h == h) {
            return Ok((c.apply(gray)?, c));
        }

        // Cold: measure the folder to decide whether this resolution is
        // stretched. The colour test needs the original (non-grayscale) frame.
        let Some(folder) = detect_folder_aspect(bgra_frame, w, h)? else {
            // No folder on this frame -- can't calibrate yet. Match it as-is and
            // leave the cache empty so a later menu frame can calibrate.
            let calib = AspectCalibration::identity(w, h);
            return Ok((gray.try_clone()?, calib));
        };

        let mut calib = if folder.aspect > FOLDER_STRETCH_ASPECT {
            // Stretched: the picture is 4:3 squeezed wide. Trim any dark side
            // bars, then squish the remaining content back to a 4:3 width.
            let (left, right) = content_h_extent(gray, BAR_BRIGHTNESS)?;
            let crop_w = (right - left + 1).max(1);
            let target_w = (((h as f64) * TARGET_ASPECT).round() as i32).max(1);
            dbg_cv!(
                "[calibrate] {w}x{h} folder_aspect={:.3} stretched -> crop {left}+{crop_w} squish to {target_w}",
                folder.aspect
            );
            AspectCalibration { src_w: w, src_h: h, crop_x: left, crop_w, target_w, folder_rect: None }
        } else {
            // Folder is correctly proportioned (clean 4:3 or pillarboxed): no
            // correction. Cache identity so later frames skip the measurement.
            dbg_cv!("[calibrate] {w}x{h} folder_aspect={:.3} not stretched", folder.aspect);
            AspectCalibration::identity(w, h)
        };
        calib.folder_rect = Some(folder.rect);

        if let Ok(mut cache) = self.aspect_cache.lock() {
            *cache = Some(calib);
        }
        Ok((calib.apply(gray)?, calib))
    }

    /// The capture transform learned for the current source, or `None` while
    /// uncalibrated or already 4:3. The monitor feeds it back so the GPU
    /// crops+un-stretches frames directly; stable once non-`None`.
    pub fn capture_region(&self) -> Option<CaptureRegion> {
        let calib = (*self.aspect_cache.lock().ok()?)?;
        if calib.is_identity() {
            return None;
        }
        Some(calib.capture_region())
    }

    // Reads the mission number inside `rect` of native-res `gray`, sweeping
    // `scales` (implied first) and stopping at the first confident digit. Returns
    // the match and its scale; result coordinates are relative to `rect`.
    fn read_mission(&self, gray: &Mat, rect: Rect, scales: &[f64]) -> Result<(FoundMission, f64)> {
        let region = gray.roi(rect)?;
        let mut found =
            FoundMission { mission: -1, score: GLYPH_THRESHOLD, colon_cx: -1, colon_cy: -1, colon: None, digit: None };
        let mut scale_used = scales.first().copied().unwrap_or(1.0);
        // Sweep scales sequentially to preserve the early-exit: a native-res
        // mission read is expensive, so the implied scale (tried first) must
        // short-circuit. Parallelism lives inside `find_mission_from_colons`.
        for &scale in scales {
            let glyphs = self.scaled_glyphs(scale)?;
            let f = find_mission_from_colons(&region, &glyphs.colon, &glyphs.digits)?;
            dbg_cv!(
                "[mission] scale={scale:.3} m={} score={:.3} cx={} cy={}",
                f.mission,
                f.score,
                f.colon_cx,
                f.colon_cy
            );
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

    // Detects the mission-select grid by matching its film-strip divider in the
    // inter-row band. Sweeps `scales` (implied first), stopping at the first to
    // clear the threshold. Returns the peak correlation found.
    fn detect_levels(&self, frame: &Mat, scales: &[f64]) -> Result<Option<MatchRect>> {
        if self.levels.empty() {
            return Ok(None);
        }
        let (rx, ry, rw, rh) = LEVELS_REGION;
        let x0 = (frame.cols() as f64 * rx) as i32;
        let y0 = (frame.rows() as f64 * ry) as i32;
        let w = ((frame.cols() as f64 * rw) as i32).min(frame.cols() - x0).max(1);
        let h = ((frame.rows() as f64 * rh) as i32).min(frame.rows() - y0).max(1);
        // Own the ROI so the parallel scale closures share a plain `&Mat`.
        let region = frame.roi(Rect::new(x0, y0, w, h))?.try_clone()?;
        let region = &region;

        // Score the divider at every scale in parallel (the dominant cost on a
        // rejected/unknown frame, where no scale clears the bar and all run).
        let scores: Vec<Result<Option<MatchRect>>> =
            par_map(scales.len(), |i| best_match(region, &scaled(&self.levels, scales[i])?));

        // Replay the sequential early-exit selection so the result matches the
        // serial version exactly: the first scale to clear the threshold wins.
        let mut best: Option<MatchRect> = None;
        for (i, r) in scores.into_iter().enumerate() {
            let Some(mut r) = r? else { continue };
            r = r.offset(x0, y0);
            dbg_cv!("[levels] scale={:.3} score={:.3}", scales[i], r.score);
            if best.is_none_or(|b| r.score > b.score) {
                best = Some(r);
            }
            if best.is_some_and(|b| b.score >= LEVELS_THRESHOLD) {
                break;
            }
        }
        Ok(best)
    }

    fn detect_start_language(&self, frame: &Mat, scale: f64) -> Result<Option<(&'static str, MatchRect)>> {
        let search_rect = fractional_rect(frame.cols(), frame.rows(), LANGUAGE_START_REGION);
        let region = frame.roi(search_rect)?.try_clone()?;
        let en = best_match(&region, &scaled(&self.language_start_en, scale)?)?;
        let jp = best_match(&region, &scaled(&self.language_start_jp, scale)?)?;
        let en_score = en.map_or(-1.0, |r| r.score);
        let jp_score = jp.map_or(-1.0, |r| r.score);
        dbg_cv!("[language] start en={en_score:.3} jp={jp_score:.3}");

        let (lang, rect, score, other) =
            if en_score >= jp_score { ("en", en, en_score, jp_score) } else { ("jp", jp, jp_score, en_score) };
        if score >= LANGUAGE_START_THRESHOLD && score - other >= LANGUAGE_START_MARGIN {
            Ok(rect.map(|r| (lang, r.offset(search_rect.x, search_rect.y))))
        } else {
            Ok(None)
        }
    }

    // Identifies the overlay screen by matching each screen's banner word (and
    // report screens' status values) at the header-established scale; strongest
    // above threshold wins, else `Unknown`. Off-scale misses trigger a small sweep.
    fn classify_screen(&self, frame: &Mat, scale: f64) -> Result<(Screen, Option<MatchRect>)> {
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
        let mut best_rect: Option<MatchRect> = None;
        let mut best_score_v = -1.0;
        let search =
            |scale: f64, best: &mut Screen, best_rect: &mut Option<MatchRect>, best_score_v: &mut f64| -> Result<()> {
                // Match all eight banner/status templates for this scale in parallel,
                // then fold in index order so ties resolve exactly as the serial
                // version did.
                let scores: Vec<Result<Option<MatchRect>>> =
                    par_map(candidates.len(), |i| best_match(candidates[i].2, &scaled(candidates[i].1, scale)?));
                for (i, r) in scores.into_iter().enumerate() {
                    let Some(r) = r? else { continue };
                    let s = r.score;
                    dbg_cv!("[screen] {:?} scale={scale:.3} score={s:.3}", candidates[i].0);
                    if s > *best_score_v {
                        *best_score_v = s;
                        *best = candidates[i].0;
                        let (rx, ry, _, _) = if i < 4 { SCREEN_BANNER_REGION } else { SCREEN_STATUS_REGION };
                        let ox = (frame.cols() as f64 * rx) as i32;
                        let oy = (frame.rows() as f64 * ry) as i32;
                        *best_rect = Some(r.offset(ox, oy));
                    }
                }
                Ok(())
            };

        search(scale, &mut best, &mut best_rect, &mut best_score_v)?;
        // Recover an off-scale overlay only when the implied scale found nothing.
        // The long banner/status words are more scale-sensitive than the glyphs
        // that fix `scale`, so sweep 2.5% steps to +/-10%, nearest first, cheaply.
        if best_score_v < SCREEN_THRESHOLD {
            for m in [0.975, 1.025, 0.95, 1.05, 0.925, 1.075, 0.90, 1.10] {
                search(scale * m, &mut best, &mut best_rect, &mut best_score_v)?;
                if best_score_v >= SCREEN_THRESHOLD {
                    break;
                }
            }
        }

        dbg_cv!("[screen] => {best:?} ({best_score_v:.3})");
        Ok(if best_score_v >= SCREEN_THRESHOLD { (best, best_rect) } else { (Screen::Unknown, None) })
    }

    /// # Safety
    /// `data` must point to at least `w * h * 4` readable bytes of 8-bit BGRA
    /// pixel data that stays valid for the duration of the call.
    pub unsafe fn match_level_from_raw_bytes(&self, data: *mut u8, w: u32, h: u32) -> Result<LevelMatch> {
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

    /// Decodes an encoded image (PNG/BMP/etc.) and matches it. Used by the
    /// developer tool to match a dumped frame dropped in from disk. Returns the
    /// match plus the decoded image's dimensions (annotations are in its coords).
    pub fn match_level_from_encoded_image(&self, bytes: &[u8]) -> Result<(LevelMatch, u32, u32)> {
        let buf = Mat::from_slice(bytes)?;
        let bgr = imgcodecs::imdecode(&buf, imgcodecs::IMREAD_COLOR)?;
        if bgr.empty() {
            return Err(opencv::Error::new(core::StsError, "could not decode image".to_owned()));
        }
        let mut bgra = Mat::default();
        imgproc::cvt_color_def(&bgr, &mut bgra, imgproc::COLOR_BGR2BGRA)?;
        let (w, h) = (bgra.cols() as u32, bgra.rows() as u32);
        let level_match = self.match_level_from_bgra_frame(&bgra)?;
        Ok((level_match, w, h))
    }

    pub fn match_level_from_bgra_frame(&self, bgra_frame: &impl ToInputArray) -> Result<LevelMatch> {
        let mut result = LevelMatch {
            screen: Screen::Unknown,
            mission: -1,
            part: -1,
            difficulty: -1,
            detected_lang: None,
            times: None,
            raw_times: Vec::new(),
            match_regions: Vec::new(),
            annotation_sets: Vec::new(),
            runtime_ms: 0.0,
        };
        let mut match_regions = Vec::new();
        let mut search_regions = Vec::new();
        // Per-digit diagnostic boxes (work coords) from the time reader, mapped to a
        // developer "Time digits" annotation set below when diagnostics are on.
        let mut time_digit_diag: Vec<(String, MatchRect)> = Vec::new();
        let mut timer = PhaseTimer::new();

        // Convert the BGRA frame to grayscale once; every template is matched
        // against this single-channel frame.
        let mut gray = Mat::default();
        imgproc::cvt_color_def(bgra_frame, &mut gray, imgproc::COLOR_BGRA2GRAY)?;

        // Restore a 4:3 picture that an HDMI converter stretched wide, so glyphs
        // regain the proportions templates expect. Calibrated once per resolution
        // off the folder; a no-op on clean 4:3 or pillarboxed grabs.
        let (gray, calib) = self.calibrate_aspect(bgra_frame, &gray)?;
        let folder_region = self.folder_annotation(calib);

        // Match cost grows with frame area, so downscale tall frames to a fixed
        // working height to bound it. `gray` keeps native res for the mission-digit
        // read; `frame` is the downscaled copy for the blur-tolerant matches.
        let frame = if gray.rows() > WORK_HEIGHT {
            let scale = WORK_HEIGHT as f64 / gray.rows() as f64;
            let w = ((gray.cols() as f64 * scale).round() as i32).max(1);
            let mut out = Mat::default();
            imgproc::resize(&gray, &mut out, Size::new(w, WORK_HEIGHT), 0.0, 0.0, imgproc::INTER_AREA)?;
            out
        } else {
            gray.try_clone()?
        };
        let mapper = RegionMapper::from_frames(calib, &gray, &frame);
        timer.lap("grayscale+downscale");

        // Scales to try are derived from the frame height, so each resolution
        // only searches the handful of scales near its own.
        let scales = candidate_scales(frame.rows());

        // If a previous frame at this resolution resolved the overlay scale, reuse
        // it so the gate and mission searches try just that one scale. The first
        // overlay frame still pays the full search to learn it (stored at the end).
        let (src_w, src_h) = (gray.cols(), gray.rows());
        let hint = self.scale_cache.lock().ok().and_then(|c| *c).filter(|c| c.src_w == src_w && c.src_h == src_h);
        let gate_scales: Vec<f64> = match hint {
            Some(c) => vec![c.overlay_scale],
            None => scales.clone(),
        };

        // Entry gate: the stats overlay (briefing and stats screens) carries a
        // stack of left-aligned header rows ending in colons. Two strong colons
        // admit both screens, reject gameplay, and fix the scale reused below.
        let header = detect_header_colons(
            &frame,
            &self.colon,
            &gate_scales,
            TIME_GATE_COLON_THRESHOLD,
            (HEADER_REGION_X, HEADER_REGION_Y, HEADER_REGION_W, HEADER_REGION_H),
        )?;
        self.push_work_search_region(
            &mut search_regions,
            &mapper,
            "header colon gate",
            fractional_rect(
                frame.cols(),
                frame.rows(),
                (HEADER_REGION_X, HEADER_REGION_Y, HEADER_REGION_W, HEADER_REGION_H),
            ),
        );
        let has_header = header.count >= 2 && header.peak >= TIME_GATE_STRONG_COLON;
        if self.diagnostics {
            for (i, r) in header_colon_regions(&frame, &self.colon, header.scale, TIME_GATE_COLON_THRESHOLD)?
                .into_iter()
                .enumerate()
            {
                self.push_work_region(&mut match_regions, &mapper, format!("header colon {}", i + 1), r);
            }
        }
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
            // No header colons: gameplay, a transition, or the mission-select
            // grid. Try to recognize the grid by its film-strip divider, reusing
            // the cached overlay scale when known (a cold session sweeps the ladder).
            self.push_work_search_region(
                &mut search_regions,
                &mapper,
                "levels divider search",
                fractional_rect(frame.cols(), frame.rows(), LEVELS_REGION),
            );
            let levels_match = self.detect_levels(&frame, &gate_scales)?;
            let levels_score = levels_match.map_or(-1.0, |m| m.score);
            if let Some(r) = levels_match.filter(|m| m.score >= LEVELS_THRESHOLD) {
                result.screen = Screen::Levels;
                self.push_work_region(&mut match_regions, &mapper, "levels divider", r);
            }
            dbg_cv!("[gate] no header; levels_score={levels_score:.3} => {:?}", result.screen);
            timer.lap("levels detect");
            result.match_regions = match_regions;
            result.annotation_sets = annotation_sets(&result.match_regions, search_regions, folder_region, Vec::new());
            result.runtime_ms = timer.start().elapsed().as_secs_f64() * 1000.0;
            return Ok(result);
        }

        self.push_work_search_region(
            &mut search_regions,
            &mapper,
            "language start tab search",
            fractional_rect(frame.cols(), frame.rows(), LANGUAGE_START_REGION),
        );
        if let Some((detected_lang, rect)) = self.detect_start_language(&frame, header.scale)? {
            result.detected_lang = Some(detected_lang.to_owned());
            self.push_work_region(&mut match_regions, &mapper, format!("language {detected_lang} start tab"), rect);
            if detected_lang != self.lang {
                dbg_cv!("[language] configured={} detected={detected_lang}; rejecting wrong-language frame", self.lang);
                result.match_regions = match_regions;
                result.annotation_sets = annotation_sets(&result.match_regions, search_regions, folder_region, Vec::new());
                result.runtime_ms = timer.start().elapsed().as_secs_f64() * 1000.0;
                return Ok(result);
            }
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
        self.push_work_search_region(
            &mut search_regions,
            &mapper,
            "mission/part/difficulty label search",
            Rect::new(
                0,
                0,
                (frame.cols() as f64 * LABEL_REGION_W) as i32,
                (frame.rows() as f64 * LABEL_REGION_H) as i32,
            ),
        );

        // Read the mission number on the NATIVE-resolution frame: anchor on ':'
        // and take the strongest digit to its left, in a small top-left box. The
        // scale is swept cold because the digit (unlike the colon) is scale-fussy.
        let mission_scales: Vec<f64> = match hint {
            Some(c) => vec![c.mission_scale],
            None => candidate_scales(gray.rows()),
        };
        // Search box. Cold: the header band (excludes title and rows below). Warm:
        // a tight box around the cached mission colon, so the digit is read in a
        // few hundred pixels instead of the whole header (the bulk of native cost).
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
                // The cached point is the colon centre. One colon-height each way
                // vertically absorbs jitter without admitting the difficulty/part
                // rows; two heights left cover the single mission digit.
                let x0 = (c.mission_cx - ch * 2).max(0);
                let y0 = (c.mission_cy - ch).max(0);
                let x1 = (c.mission_cx + ch).min(gray.cols());
                let y1 = (c.mission_cy + ch).min(gray.rows());
                Rect::new(x0, y0, (x1 - x0).max(1), (y1 - y0).max(1))
            }
            _ => header_box(),
        };
        self.push_corrected_search_region(&mut search_regions, &mapper, "mission digit search", mission_rect);

        let (mut found, mut mission_scale) = self.read_mission(&gray, mission_rect, &mission_scales)?;
        let mut mission_rect = mission_rect;
        // Warm box missed (capture jitter / overlay shifted): retry the full
        // header band at the cached scale before giving up.
        if found.mission < 0 && hint.is_some() {
            mission_rect = header_box();
            self.push_corrected_search_region(&mut search_regions, &mapper, "mission digit retry search", mission_rect);
            let (f, s) = self.read_mission(&gray, mission_rect, &mission_scales)?;
            found = f;
            mission_scale = s;
        }
        result.mission = found.mission;
        if let Some(r) = found.colon.map(|r| r.offset(mission_rect.x, mission_rect.y)) {
            self.push_corrected_region(&mut match_regions, &mapper, "mission colon", r);
        }
        if let Some(r) = found.digit.map(|r| r.offset(mission_rect.x, mission_rect.y)) {
            self.push_corrected_region(&mut match_regions, &mapper, format!("mission {}", found.mission), r);
        }

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

        // The difficulty row sits one glyph-line above the mission row, the part
        // row one below. Anchoring each label search to a short band around the
        // mission row (three colon-heights each way) cuts label matching severalfold.
        let colon_h = (self.colon.rows() as f64 * global_scale).round() as i32;
        let mission_cy = mission_cy_frame;
        let pad = ((colon_h as f64) * 0.4) as i32;

        let mut part_rect = None;
        result.part = if mission_cy >= 0 && colon_h > 0 {
            let y0 = (mission_cy + pad).clamp(0, label_region.rows());
            let y1 = (mission_cy + colon_h * 3).clamp(0, label_region.rows());
            if y1 - y0 >= 2 {
                self.push_work_search_region(
                    &mut search_regions,
                    &mapper,
                    "part label band",
                    Rect::new(0, y0, label_region.cols(), y1 - y0),
                );
            }
            let part = best_label_in_band_match(
                &label_region,
                &self.parts,
                global_scale,
                LABEL_THRESHOLD,
                mission_cy + pad,
                mission_cy + colon_h * 3,
            )?;
            if let Some((part, r)) = part {
                part_rect = Some(r);
                part
            } else {
                -1
            }
        } else {
            -1
        };
        // Fall back to a full-region scale sweep when the anchored band misses,
        // which also recovers the true scale on off-scale captures.
        if result.part < 0 {
            self.push_work_search_region(
                &mut search_regions,
                &mapper,
                "part label fallback search",
                Rect::new(0, 0, label_region.cols(), label_region.rows()),
            );
            let (part, part_scale, rect) =
                best_label_match_over_scales(&label_region, &self.parts, &scales, LABEL_THRESHOLD)?;
            if part >= 0 {
                result.part = part;
                part_rect = rect;
                global_scale = part_scale;
                dbg_cv!("[scale recovery] part={part} scale={part_scale:.3}");
            }
        }
        if let Some(r) = part_rect {
            self.push_work_region(&mut match_regions, &mapper, format!("part {}", result.part), r);
        }
        timer.lap("part label");

        let colon_h = (self.colon.rows() as f64 * global_scale).round() as i32;
        let mut difficulty_rect = None;
        let mut difficulty_label = if mission_cy >= 0 && colon_h > 0 {
            let y0 = (mission_cy - colon_h * 3).clamp(0, label_region.rows());
            let y1 = (mission_cy - pad).clamp(0, label_region.rows());
            if y1 - y0 >= 2 {
                self.push_work_search_region(
                    &mut search_regions,
                    &mapper,
                    "difficulty label band",
                    Rect::new(0, y0, label_region.cols(), y1 - y0),
                );
            }
            let difficulty = best_label_in_band_match(
                &label_region,
                &self.diffs,
                global_scale,
                LABEL_THRESHOLD,
                mission_cy - colon_h * 3,
                mission_cy - pad,
            )?;
            if let Some((difficulty, r)) = difficulty {
                difficulty_rect = Some(r);
                difficulty
            } else {
                -1
            }
        } else {
            -1
        };
        if difficulty_label < 0 {
            self.push_work_search_region(
                &mut search_regions,
                &mapper,
                "difficulty label fallback search",
                Rect::new(0, 0, label_region.cols(), label_region.rows()),
            );
            if let Some((difficulty, r)) = best_label_match(&label_region, &self.diffs, global_scale, LABEL_THRESHOLD)?
            {
                difficulty_label = difficulty;
                difficulty_rect = Some(r);
            }
        }
        result.difficulty = if difficulty_label >= 0 { difficulty_label.saturating_sub(1) } else { -1 };
        if let Some(r) = difficulty_rect {
            self.push_work_region(&mut match_regions, &mapper, format!("difficulty {}", result.difficulty), r);
        }
        timer.lap("difficulty label");

        // Locate the digit and colon glyphs at the same scale.
        let glyphs = self.scaled_glyphs(global_scale)?;
        let colon_tmpl = &glyphs.colon;
        let digit_tmpls = &glyphs.digits;
        let digit_width_sum: i32 = digit_tmpls.iter().map(|t| t.cols()).sum();
        timer.lap("load glyph templates");

        // Identify the overlay screen from its banner / status value. Only stats
        // screens carry timed rows, so reading the screen lets the time search be
        // skipped elsewhere (avoiding objectives/results being mis-read as times).
        self.push_work_search_region(
            &mut search_regions,
            &mapper,
            "screen banner search",
            fractional_rect(frame.cols(), frame.rows(), SCREEN_BANNER_REGION),
        );
        self.push_work_search_region(
            &mut search_regions,
            &mapper,
            "screen status search",
            fractional_rect(frame.cols(), frame.rows(), SCREEN_STATUS_REGION),
        );
        let (screen, screen_rect) = self.classify_screen(&frame, global_scale)?;
        result.screen = screen;
        if let Some(r) = screen_rect {
            self.push_work_region(&mut match_regions, &mapper, format!("screen {}", screen.as_str()), r);
        }
        timer.lap("screen classify");

        // Read the raw times off the overlay (top-to-bottom), then classify them
        // into run / target / best using the level's mission/part/difficulty.
        let found_times: Vec<FoundTime> = if screen != Screen::Stats || colon_tmpl.empty() || digit_width_sum == 0 {
            Vec::new()
        } else {
            self.push_work_search_region(
                &mut search_regions,
                &mapper,
                "time colon search",
                fractional_rect(
                    frame.cols(),
                    frame.rows(),
                    (COLON_REGION_X, COLON_REGION_Y, COLON_REGION_W, COLON_REGION_H),
                ),
            );
            find_times_band(&frame, &glyphs, self.diagnostics.then_some(&mut time_digit_diag))?
        };
        for t in &found_times {
            self.push_work_region(&mut match_regions, &mapper, format!("time {}", format_seconds(t.seconds)), t.colon);
        }
        // Map the per-digit diagnostic boxes (work coords) into a source-space set
        // so the developer overlay shows where each digit was read from.
        let time_digits: Vec<AnnotationRect> = time_digit_diag
            .iter()
            .map(|(label, rect)| {
                let region = mapper.work_to_source(*rect);
                AnnotationRect { label: label.clone(), x: region.x, y: region.y, w: region.w, h: region.h, score: Some(region.score) }
            })
            .collect();
        let times: Vec<i32> = found_times.into_iter().map(|t| t.seconds).collect();
        result.times = ge::Times::classify(result.mission, result.part, result.difficulty, &times);
        result.raw_times = times;
        timer.lap("time assembly");

        // Learn the scale from this fully-resolved overlay (slow path only) so
        // later frames at this resolution fast-path the scale search. Require
        // every header marker so a partial match never poisons the cache.
        if hint.is_none()
            && has_overlay_markers(&result)
            && let Ok(mut cache) = self.scale_cache.lock()
        {
            *cache = Some(ScaleCache {
                src_w,
                src_h,
                overlay_scale: global_scale,
                mission_scale,
                mission_cx,
                mission_cy: mission_cy_native,
            });
            dbg_cv!(
                "[scale cache] stored overlay={global_scale:.3} mission={mission_scale:.3} colon=({mission_cx},{mission_cy_native}) for {src_w}x{src_h}"
            );
        }

        reject_untrusted_screen(&mut result);

        result.runtime_ms = timer.start().elapsed().as_secs_f64() * 1000.0;
        result.match_regions = match_regions;
        result.annotation_sets = annotation_sets(&result.match_regions, search_regions, folder_region, time_digits);

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../cv_templates");

    // Decoding + matching an encoded image (the developer upload path) reads the
    // same result as the file-based matcher; uses a committed flicker fixture.
    #[test]
    fn match_level_from_encoded_image_decodes_and_matches() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/screenshots-rt4kce/en - stats - 3 - Agent - 0028_0500_0028 - flicker-004.png"
        );
        let bytes = std::fs::read(path).expect("read fixture");
        let matcher = CvMatcher::new("en", TEMPLATES_DIR).expect("matcher");
        let (m, w, h) = matcher.match_level_from_encoded_image(&bytes).expect("decode+match");
        assert!(w > 0 && h > 0, "decoded dimensions");
        assert_eq!(m.screen, Screen::Stats);
        assert_eq!(m.times.map(|t| t.best_time), Some(Some(28)));
    }

    fn level_match(screen: Screen, mission: i32, part: i32, difficulty: i32, raw_times: Vec<i32>) -> LevelMatch {
        LevelMatch {
            screen,
            mission,
            part,
            difficulty,
            detected_lang: None,
            times: ge::Times::classify(mission, part, difficulty, &raw_times),
            raw_times,
            match_regions: Vec::new(),
            annotation_sets: Vec::new(),
            runtime_ms: 0.0,
        }
    }

    #[test]
    fn overlay_screens_with_complete_markers_remain_trusted() {
        let cases = [
            (Screen::Start, Vec::new()),
            (Screen::Stats, vec![62]),
            (Screen::Complete, Vec::new()),
            (Screen::Failed, Vec::new()),
            (Screen::Abort, Vec::new()),
            (Screen::Kia, Vec::new()),
        ];

        for (screen, raw_times) in cases {
            let mut result = level_match(screen, 1, 1, ge::AGENT, raw_times);

            reject_untrusted_screen(&mut result);

            assert_eq!(result.screen, screen, "{screen:?} should remain trusted with all markers");
        }
    }

    #[test]
    fn overlay_screens_are_rejected_when_any_required_marker_is_missing() {
        let screens = [Screen::Start, Screen::Stats, Screen::Complete, Screen::Failed, Screen::Abort, Screen::Kia];
        let marker_cases = [(-1, 1, ge::AGENT), (1, -1, ge::AGENT), (1, 1, -1)];

        for screen in screens {
            for (mission, part, difficulty) in marker_cases {
                let raw_times = if screen == Screen::Stats { vec![62] } else { Vec::new() };
                let mut result = level_match(screen, mission, part, difficulty, raw_times);

                reject_untrusted_screen(&mut result);

                assert_eq!(result.screen, Screen::Unknown, "{screen:?} should reject incomplete markers");
                assert_eq!(result.raw_times, Vec::<i32>::new());
                assert_eq!(result.times, None);
            }
        }
    }

    #[test]
    fn stats_screen_is_rejected_without_a_readable_run_time() {
        let mut result = level_match(Screen::Stats, 1, 1, ge::AGENT, Vec::new());

        reject_untrusted_screen(&mut result);

        assert_eq!(result.screen, Screen::Unknown);
    }

    #[test]
    fn non_overlay_screens_do_not_require_header_markers() {
        for screen in [Screen::Opts007, Screen::Select, Screen::Levels, Screen::Unknown] {
            let mut result = level_match(screen, -1, -1, -1, Vec::new());

            reject_untrusted_screen(&mut result);

            assert_eq!(result.screen, screen, "{screen:?} should not require mission/part/difficulty markers");
        }
    }
}
