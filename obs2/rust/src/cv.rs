// Standalone CLI for exercising the GoldenEye level matcher outside of OBS.
//
//   test_match <lang> path/to/screenshot.png [templates_dir]
//
// Loads the given image, converts it to the BGRA layout the plugin feeds the
// matcher, runs the matcher, and prints the match result to stdout. `lang` is
// a template filename prefix such as "en" or "jp", and
// `templates_dir` defaults to the cv_templates/ directory that ships
// alongside obs2/.
//
// This is a Rust port of obs2/test_match.cpp + obs2/cv_wrapper.cpp, using the
// `opencv` crate instead of binding to OpenCV directly.

use std::sync::{Mutex, OnceLock};
use std::thread;

use opencv::core::{self, Mat, Rect, Size, ToInputArray};
use opencv::prelude::*;
use opencv::{Result, imgcodecs, imgproc};
use serde::Serialize;

use crate::ge;
use crate::timer::PhaseTimer;

// Cached count of usable cores. OpenCV here is built without TBB/OpenMP, so each
// `match_template` pins a single core; the per-scale / per-template matches that
// dominate match time are independent and are spread across the spare cores with
// `par_map`. Queried once -- the value is fixed for the process.
fn parallelism() -> usize {
    static N: OnceLock<usize> = OnceLock::new();
    *N.get_or_init(|| thread::available_parallelism().map(|p| p.get()).unwrap_or(1))
}

// Maps `f` over `0..n`, returning the results in index order. The work is split
// into contiguous chunks run on scoped OS threads so the independent OpenCV
// template matches in a sweep execute concurrently (a near-linear speedup on
// multicore, since each match is single-threaded). Tiny `n` or a single-core
// machine falls back to a serial map so the thread setup is only paid when it
// wins. Order is preserved, so callers can replay any sequential selection
// (e.g. early-exit-at-first-hit) over the results and get an identical answer.
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
// Language detection uses the side tab on the level-start briefing. The tab is
// short, static, and visually distinct between the English and Japanese ROMs,
// so it can reject a wrong ROM/template language before a
// same-shaped banner in the wrong language is misclassified as another screen.
const LANGUAGE_START_THRESHOLD: f64 = 0.82;
const LANGUAGE_START_MARGIN: f64 = 0.12;
// The mission-select grid ("levels" screen) carries none of the header colons
// the other overlays share, so the entry gate rejects it. It is instead
// recognized by the distinctive film-strip divider that separates its four
// rows of level thumbnails: a tan horizontal bar flanked above and below by a
// row of sprocket-hole dots. That divider is part of the static film-strip
// frame, so it is present (and identical between en/jp) even while the
// thumbnails are still loading in -- which the report/start overlays and busy
// gameplay frames never reproduce. The template is matched across the band that
// holds the three inner dividers; the strongest correlation above this
// threshold classifies the frame as `Levels`.
const LEVELS_THRESHOLD: f64 = 0.68;
// (x, y, w, h) as fractions of the frame: a band over the left half of the
// film strip spanning the first two inter-row dividers (~0.27 and ~0.50 of the
// frame at every capture resolution). Two dividers give redundancy -- the
// floating selection crosshair can sit over one mid-transition -- while a tight
// band keeps the single template match cheap. The right tab ("PREVIOUS") and
// the outer margins are excluded.
const LEVELS_REGION: (f64, f64, f64, f64) = (0.04, 0.20, 0.52, 0.42);

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
// Exposed so the live capture can downscale to the same height up front (the
// GPU does it for free), making this internal downscale a no-op on those frames.
pub const WORK_HEIGHT: i32 = 480;

// GoldenEye always renders a 4:3 image. Some HDMI converters take that 4:3
// signal and stretch it to fill a 16:9 frame, so every on-screen glyph comes
// out wider than it is tall. The matcher derives a single uniform scale from
// the frame height and matches templates at that one scale, so a horizontally
// stretched frame defeats it: the glyph width no longer matches the template.
//
// Every overlay the matcher reads (level briefing, post-mission stats, the
// report screens, the options/difficulty screens) is drawn on the same big
// manilla folder. That folder is the one object always on screen whose true
// proportions are known, so it is used to calibrate: locate the folder, measure
// its width:height, and if it is wider than the folder ever is at 4:3 the
// picture has been stretched. The calibration (a horizontal squish back to 4:3)
// is learned once per source resolution and reused for every later frame --
// including frames with no folder of their own (the mission-select grid), which
// inherit the resolution's transform. See [`CvMatcher::calibrate_aspect`].
const TARGET_ASPECT: f64 = 4.0 / 3.0;
// The manilla folder's width:height measures ~1.20-1.26 across clean 4:3
// captures (the spread is overscan in the vertical extent, not real shape
// change). A folder wider than this is the tell-tale of a horizontally
// stretched picture; the threshold sits comfortably between that native band
// and the ~1.66 a 16:9-stretched folder measures.
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
// A column whose mean brightness is below this counts as a (black) pillarbox
// bar rather than content. Real captures put their bars at ~0 while the
// GoldenEye background texture never falls near it, so a stretched frame's
// content extent is trimmed of any bars before it is squished back to 4:3.
const BAR_BRIGHTNESS: f64 = 24.0;

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

// Horizontal extent [left, right] (inclusive) of the non-bar content in a
// grayscale frame: the first and last columns whose mean brightness rises above
// `bar_brightness`. Dark pillarbox bars flanking the picture are trimmed; a
// frame with no bars yields the full width. Returns the full width if every
// column reads as dark (degenerate frame), so callers never act on it.
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

// Measures the width:height of the manilla folder in a `w`x`h` BGRA frame, or
// `None` when no folder-like region is present (gameplay, the mission-select
// grid, a transition). The folder is the large warm (high-red, bright) block
// that backs every menu overlay; it is isolated with a colour+brightness mask
// and its extent read off the row/column projections of that mask, so interior
// dark elements (the briefing photo, the stats text) don't shrink the box. The
// frame is downscaled first -- the aspect is scale-invariant and this only runs
// once per resolution, so working small keeps the cold-frame calibration cheap.
fn detect_folder_aspect(bgra_frame: &impl ToInputArray, w: i32, h: i32) -> Result<Option<f64>> {
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
    dbg_cv!("[folder] box {fw}x{fh} on {dw}x{dh} aspect={:.3}", fw / fh);
    Ok(Some(fw / fh))
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

#[derive(Clone, Copy)]
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
    /// The raw times read off the overlay in top-to-bottom order, before
    /// classification -- the unclassified source `times` is derived from. Empty
    /// on screens with no timed rows. Kept for the digit-matching test harness,
    /// which asserts every rendered row was read correctly; production code uses
    /// the classified `times` instead.
    pub raw_times: Vec<i32>,
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
    // Own the (small) region so the per-template closures can share a `&Mat`
    // across the scoped threads, then match every label template in parallel.
    let frame = frame.try_clone()?;
    let frame = &frame;
    let scores: Vec<Result<f64>> = par_map(templates.len(), |i| best_score(frame, &scaled(&templates[i], scale)?));

    let mut best = -1;
    let mut best_score_v = threshold;
    for (i, s) in scores.into_iter().enumerate() {
        let s = s?;
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

    // Replay the original sequential selection over the parallel results so the
    // chosen scale is identical to the serial version: prefer the scale that
    // resolves the most colons (the full header stack), breaking ties on peak
    // correlation, and stop at the first scale that lands a confident header
    // row pair (no later scale can change the outcome).
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

    // Anchor only on confident colons. A real header colon clears ~0.9, while
    // the low glyph threshold would also match noise all over a textured
    // background -- each spurious hit then triggers a 10-digit search and an
    // O(n^2) suppression, which is what made the per-scale mission search slow.
    let mut colons = Vec::new();
    collect_detections(label_region, colon_tmpl, COLON_ANCHOR_THRESHOLD, 0, &mut colons)?;
    let colons = suppress(colons, colon_w, colon_h, 0.5);

    let band_pad_x = digit_w * 2;
    let band_pad_y = digit_h;

    // Each (colon, digit) pair is an independent template search. At native
    // resolution the digit templates are large and there can be several anchor
    // colons, so this is the bulk of the mission cost; fan the pairs across the
    // cores and reduce to the single strongest digit immediately left of a
    // colon afterwards. Each work item returns its own best candidate so the
    // final reduction reproduces the serial "highest-scoring digit wins".
    let work: Vec<(usize, usize)> = (0..colons.len()).flat_map(|c| (1..=9).map(move |v| (c, v))).collect();
    let partials: Vec<Result<Option<FoundMission>>> = par_map(work.len(), |k| {
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
                });
            }
        }
        Ok(best)
    });

    let mut best = FoundMission { mission: -1, score: -1.0, colon_cx: -1, colon_cy: -1 };
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
        for (v, tmpl) in digit_tmpls.iter().enumerate().take(10) {
            let mut bucket = Vec::new();
            collect_detections(&roi, tmpl, GLYPH_THRESHOLD, v as i32, &mut bucket)?;
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

    // A single time-colon can register twice when a side-lobe peak survives the
    // colon suppression -- likeliest at small template scales, where the colon is
    // only a few pixels wide and the suppression radius (~colon_w) drops below the
    // side-lobe offset. Both detections anchor the same row and read the same
    // digits, yielding a duplicate time. Collapse times whose colons sit within a
    // glyph of each other; two genuine times sharing a row ("Target: .. (Best
    // Time: ..)") have colons many digit-widths apart and are preserved.
    // The vertical radius stays well under one row's height: adjacent rows share
    // the same value-colon x (the times align to a tab stop), so a tall threshold
    // would fold two real rows into one. A side-lobe duplicate sits within a
    // couple of pixels of its twin, far inside this bound.
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

// The aspect correction learned for a source resolution: the horizontal window
// of the frame that holds the 4:3 picture, and the width that window is resized
// to (height is never touched -- the converters that stretch only ever stretch
// horizontally). Learned once from the first frame that shows a manilla folder
// and reused for every later frame at the same resolution, so a stretched
// mission-select grid (which has no folder of its own) is still corrected.
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
}

// The learned aspect correction expressed as a source-relative capture
// transform: the sub-rectangle of the source that holds the 4:3 picture
// (fractions in [0, 1]) and the aspect ratio it should be resized to. The
// monitor hands this to the capture layer so the GPU crops and un-stretches
// future frames in one pass, instead of the matcher redoing it on the CPU every
// frame. Fractions are resolution-independent, so they apply regardless of the
// height the capture downscales to.
#[derive(Clone, Copy, Debug)]
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
        AspectCalibration { src_w, src_h, crop_x: 0, crop_w: src_w, target_w: src_w }
    }

    // As a source-relative capture transform. Horizontal crop only -- the
    // converters stretch horizontally, so the full height is always kept. The
    // crop is a fraction of the frame width, which equals the same fraction of
    // the source width (the capture downscale preserves the horizontal aspect).
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

pub struct CvMatcher {
    lang: String,
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
}

impl CvMatcher {
    pub fn new(lang: &str, templates_dir: &str) -> Result<Self> {
        // Pin OpenCV's own parallel backend to a single thread. On macOS that
        // backend is GCD, which fans every `match_template` out across the GCD
        // thread pool. We instead drive parallelism explicitly with `par_map`
        // (one thread per independent template/scale match), so leaving OpenCV
        // multi-threaded too would oversubscribe the cores -- N concurrent
        // matches each spawning M internal threads -- and produce large
        // tail-latency spikes (frames occasionally taking 3-5x the median).
        // One match == one core keeps the per-frame time tight and predictable,
        // which is what matters for never missing a single-frame overlay.
        // `GE_CV_THREADS` (the benchmarking hook) opts out so the internal
        // backend can still be measured in isolation.
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
        })
    }

    // Returns `gray` corrected to 4:3 when the source is a stretched 4:3 picture,
    // or unchanged otherwise. The correction is learned once per source
    // resolution (the "calibration" step) and cached: on the first frame at a
    // new resolution it looks for the manilla folder and, if that folder is
    // wider than it ever is at 4:3, records the horizontal squish that restores
    // it. Frames with no folder (gameplay, the mission-select grid) don't
    // calibrate on their own but inherit a calibration learned from an earlier
    // menu frame this session -- so once any folder has been seen, the whole
    // session's frames are corrected.
    fn calibrate_aspect(&self, bgra_frame: &impl ToInputArray, gray: &Mat) -> Result<Mat> {
        let (w, h) = (gray.cols(), gray.rows());

        // Reuse the calibration already learned for this resolution.
        if let Some(c) = self.aspect_cache.lock().ok().and_then(|c| *c).filter(|c| c.src_w == w && c.src_h == h) {
            return c.apply(gray);
        }

        // Cold: measure the folder to decide whether this resolution is
        // stretched. The colour test needs the original (non-grayscale) frame.
        let Some(folder_aspect) = detect_folder_aspect(bgra_frame, w, h)? else {
            // No folder on this frame -- can't calibrate yet. Match it as-is and
            // leave the cache empty so a later menu frame can calibrate.
            return gray.try_clone();
        };

        let calib = if folder_aspect > FOLDER_STRETCH_ASPECT {
            // Stretched: the picture is 4:3 squeezed wide. Trim any dark side
            // bars, then squish the remaining content back to a 4:3 width.
            let (left, right) = content_h_extent(gray, BAR_BRIGHTNESS)?;
            let crop_w = (right - left + 1).max(1);
            let target_w = (((h as f64) * TARGET_ASPECT).round() as i32).max(1);
            dbg_cv!(
                "[calibrate] {w}x{h} folder_aspect={folder_aspect:.3} stretched -> crop {left}+{crop_w} squish to {target_w}"
            );
            AspectCalibration { src_w: w, src_h: h, crop_x: left, crop_w, target_w }
        } else {
            // Folder is correctly proportioned (clean 4:3 or pillarboxed): no
            // correction. Cache identity so later frames skip the measurement.
            dbg_cv!("[calibrate] {w}x{h} folder_aspect={folder_aspect:.3} not stretched");
            AspectCalibration::identity(w, h)
        };

        if let Ok(mut cache) = self.aspect_cache.lock() {
            *cache = Some(calib);
        }
        calib.apply(gray)
    }

    /// The capture transform learned for the current source, or `None` while the
    /// source is uncalibrated (no folder seen yet) or already 4:3 (no correction
    /// needed). The monitor feeds this back to the capture layer so the GPU
    /// crops + un-stretches future frames directly. Once a non-`None` value is
    /// returned it stays stable for the session's source resolution.
    pub fn capture_region(&self) -> Option<CaptureRegion> {
        let calib = (*self.aspect_cache.lock().ok()?)?;
        if calib.is_identity() {
            return None;
        }
        Some(calib.capture_region())
    }

    // Reads the mission number inside `rect` of the native-resolution `gray`,
    // sweeping `scales` and stopping at the first that lands a confident digit
    // (the resolution-implied scale is tried first). Returns the match and the
    // scale it was found at. Coordinates in the result are relative to `rect`.
    fn read_mission(&self, gray: &Mat, rect: Rect, scales: &[f64]) -> Result<(FoundMission, f64)> {
        let region = gray.roi(rect)?;
        let mut found = FoundMission { mission: -1, score: GLYPH_THRESHOLD, colon_cx: -1, colon_cy: -1 };
        let mut scale_used = scales.first().copied().unwrap_or(1.0);
        // Sweep scales sequentially so the early-exit is preserved: a single
        // scale's mission read is expensive at native resolution (the digit
        // templates are large), so the resolution-implied scale -- tried first
        // and almost always the right one -- must short-circuit the rest rather
        // than every scale being matched up front. The parallelism instead lives
        // inside `find_mission_from_colons`, which fans the per-digit searches of
        // the one scale that runs across the cores.
        for &scale in scales {
            let colon_tmpl = scaled(&self.colon, scale)?;
            let mut digit_tmpls = Vec::with_capacity(10);
            for v in 0..=9 {
                digit_tmpls.push(scaled(&self.digits[v], scale)?);
            }
            let f = find_mission_from_colons(&region, &colon_tmpl, &digit_tmpls)?;
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
    // band that holds the three inter-row dividers. Sweeps `scales` (the
    // resolution-implied scale first) and stops at the first that clears the
    // threshold, so an in-spec capture settles on the first try. Returns the
    // peak correlation found.
    fn detect_levels(&self, frame: &Mat, scales: &[f64]) -> Result<f64> {
        if self.levels.empty() {
            return Ok(-1.0);
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
        let scores: Vec<Result<f64>> = par_map(scales.len(), |i| best_score(region, &scaled(&self.levels, scales[i])?));

        // Replay the sequential early-exit selection so the result matches the
        // serial version exactly: the first scale to clear the threshold wins.
        let mut best = -1.0;
        for (i, s) in scores.into_iter().enumerate() {
            let s = s?;
            dbg_cv!("[levels] scale={:.3} score={s:.3}", scales[i]);
            if s > best {
                best = s;
            }
            if best >= LEVELS_THRESHOLD {
                break;
            }
        }
        Ok(best)
    }

    fn detect_start_language(&self, frame: &Mat, scale: f64) -> Result<Option<&'static str>> {
        let en = best_score(frame, &scaled(&self.language_start_en, scale)?)?;
        let jp = best_score(frame, &scaled(&self.language_start_jp, scale)?)?;
        dbg_cv!("[language] start en={en:.3} jp={jp:.3}");

        let (lang, score, other) = if en >= jp { ("en", en, jp) } else { ("jp", jp, en) };
        if score >= LANGUAGE_START_THRESHOLD && score - other >= LANGUAGE_START_MARGIN {
            Ok(Some(lang))
        } else {
            Ok(None)
        }
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
            // Match all eight banner/status templates for this scale in parallel,
            // then fold in index order so ties resolve exactly as the serial
            // version did.
            let scores: Vec<Result<f64>> =
                par_map(candidates.len(), |i| best_score(candidates[i].2, &scaled(candidates[i].1, scale)?));
            for (i, s) in scores.into_iter().enumerate() {
                let s = s?;
                dbg_cv!("[screen] {:?} scale={scale:.3} score={s:.3}", candidates[i].0);
                if s > *best_score_v {
                    *best_score_v = s;
                    *best = candidates[i].0;
                }
            }
            Ok(())
        };

        search(scale, &mut best, &mut best_score_v)?;
        // Recover an off-scale overlay only when the implied scale resolved
        // nothing; the true screen's word climbs above the bar at its real
        // scale while the others stay well below it.
        //
        // The banner/status words are long, so their correlation is far more
        // scale-sensitive than the short colon/digit/label glyphs that fix
        // `scale` upstream: those tolerate a couple percent of scale error and
        // still match, so `scale` can settle a hair off the overlay's true
        // scale. A real-source capture whose overlay sits a few percent off the
        // resolution-implied scale (composite/HDMI overscan) then peaks *between*
        // the coarse 5% steps and is missed -- e.g. an av2hdmi start screen whose
        // banner peaks at ~1.025x scores ~0.94 there but only ~0.73 at 1.0x and
        // ~0.61 at 1.05x, so the old [0.95, 1.05, ...] ladder never saw it.
        // Sweep in 2.5% steps out to +/-10% so that in-between peak is caught;
        // the nearest deviations are tried first and the search stops at the
        // first scale that clears the bar, so the recovery stays cheap.
        if best_score_v < SCREEN_THRESHOLD {
            for m in [0.975, 1.025, 0.95, 1.05, 0.925, 1.075, 0.90, 1.10] {
                search(scale * m, &mut best, &mut best_score_v)?;
                if best_score_v >= SCREEN_THRESHOLD {
                    break;
                }
            }
        }

        dbg_cv!("[screen] => {best:?} ({best_score_v:.3})");
        Ok(if best_score_v >= SCREEN_THRESHOLD { best } else { Screen::Unknown })
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

    pub fn match_level_from_bgra_frame(&self, bgra_frame: &impl ToInputArray) -> Result<LevelMatch> {
        let mut result = LevelMatch {
            screen: Screen::Unknown,
            mission: -1,
            part: -1,
            difficulty: -1,
            detected_lang: None,
            times: None,
            raw_times: Vec::new(),
            runtime_ms: 0.0,
        };
        let mut timer = PhaseTimer::new();

        // Convert the BGRA frame to grayscale once; every template is matched
        // against this single-channel frame.
        let mut gray = Mat::default();
        imgproc::cvt_color_def(bgra_frame, &mut gray, imgproc::COLOR_BGRA2GRAY)?;

        // Restore a 4:3 picture that an HDMI converter stretched to a wider
        // aspect, so the glyphs regain the proportions the templates expect.
        // Calibrated once per resolution off the manilla folder; a no-op on
        // clean 4:3 grabs and on 4:3 content pillarboxed in 16:9.
        let gray = self.calibrate_aspect(bgra_frame, &gray)?;

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
        let hint = self.scale_cache.lock().ok().and_then(|c| *c).filter(|c| c.src_w == src_w && c.src_h == src_h);
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
            // No header colons: this is gameplay, a transition, or the
            // mission-select grid (which shares none of the header rows). Try to
            // recognize the grid by its film-strip divider before giving up.
            //
            // Reuse the cached overlay scale when one is known: the film-strip
            // divider scales with the frame exactly as the stats overlay does,
            // so once any overlay frame this session has pinned the resolution's
            // scale, the grid only needs checking at that one scale. This is the
            // common steady-state path -- most gameplay frames have no header --
            // so dropping it from a full ladder sweep to a single scale keeps the
            // matcher cheap on every non-overlay frame. A cold session (no hint)
            // still sweeps the full ladder via `gate_scales == scales`.
            let levels_score = self.detect_levels(&frame, &gate_scales)?;
            if levels_score >= LEVELS_THRESHOLD {
                result.screen = Screen::Levels;
            }
            dbg_cv!("[gate] no header; levels_score={levels_score:.3} => {:?}", result.screen);
            timer.lap("levels detect");
            result.runtime_ms = timer.start().elapsed().as_secs_f64() * 1000.0;
            return Ok(result);
        }

        if let Some(detected_lang) = self.detect_start_language(&frame, header.scale)? {
            result.detected_lang = Some(detected_lang.to_owned());
            if detected_lang != self.lang {
                dbg_cv!("[language] configured={} detected={detected_lang}; rejecting wrong-language frame", self.lang);
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
            best_label_in_band(
                &label_region,
                &self.parts,
                global_scale,
                LABEL_THRESHOLD,
                mission_cy + pad,
                mission_cy + colon_h * 3,
            )?
        } else {
            -1
        };
        // Fall back to a full-region scale sweep when the anchored band misses,
        // which also recovers the true scale on off-scale captures.
        if result.part < 0 {
            let (part, part_scale) = best_label_over_scales(&label_region, &self.parts, &scales, LABEL_THRESHOLD)?;
            if part >= 0 {
                result.part = part;
                global_scale = part_scale;
                dbg_cv!("[scale recovery] part={part} scale={part_scale:.3}");
            }
        }
        timer.lap("part label");

        let colon_h = (self.colon.rows() as f64 * global_scale).round() as i32;
        let mut difficulty_label = if mission_cy >= 0 && colon_h > 0 {
            best_label_in_band(
                &label_region,
                &self.diffs,
                global_scale,
                LABEL_THRESHOLD,
                mission_cy - colon_h * 3,
                mission_cy - pad,
            )?
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

        // Read the raw times off the overlay (top-to-bottom), then classify them
        // into run / target / best using the level's mission/part/difficulty.
        let times: Vec<i32> = if screen != Screen::Stats || colon_tmpl.empty() || digit_width_sum == 0 {
            Vec::new()
        } else {
            find_times_band(&frame, &colon_tmpl, &digit_tmpls)?.into_iter().map(|t| t.seconds).collect()
        };
        if screen == Screen::Stats && times.is_empty() {
            result.screen = Screen::Unknown;
        }
        result.times = ge::Times::classify(result.mission, result.part, result.difficulty, &times);
        result.raw_times = times;
        timer.lap("time assembly");

        // Learn the scale from this fully-resolved overlay (slow path only) so
        // subsequent frames at the same resolution fast-path the scale search.
        // Require both labels found, so a partial/ambiguous match never poisons
        // the cache with a wrong scale.
        if hint.is_none()
            && result.mission >= 0
            && result.part >= 0
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

        result.runtime_ms = timer.start().elapsed().as_secs_f64() * 1000.0;

        Ok(result)
    }
}
