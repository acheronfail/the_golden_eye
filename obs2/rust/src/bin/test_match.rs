// Standalone CLI for exercising the GoldenEye level matcher outside of OBS.
//
//   test_match path/to/screenshot.png [lang] [templates_dir]
//
// Loads the given image, converts it to the BGRA layout the plugin feeds the
// matcher, runs the matcher, and prints the match result to stdout. `lang`
// defaults to "en" and `templates_dir` to the cv_templates/ directory that
// ships alongside obs2/.
//
// This is a Rust port of obs2/test_match.cpp + obs2/cv_wrapper.cpp, using the
// `opencv` crate instead of binding to OpenCV directly.

use std::env;
use std::process::ExitCode;
use std::time::Instant;

use opencv::core::{self, Mat, Rect, Size, ToInputArray};
use opencv::prelude::*;
use opencv::{Result, imgcodecs, imgproc};

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

// Region searched for the time colons: they sit in the bottom 50% of the frame
// and the middle 50% horizontally. Anchoring the (full-frame) colon search to
// this box also discards label colons ("Time:", "Accuracy:") for free.
const COLON_REGION_X: f64 = 0.25;
const COLON_REGION_W: f64 = 0.50;
const COLON_REGION_Y: f64 = 0.50;
const COLON_REGION_H: f64 = 0.50;
// Correlation needed to accept an individual digit/colon glyph.
const GLYPH_THRESHOLD: f64 = 0.78;

// Candidate scales applied to the templates when locating the stats overlay.
// The templates are authored at the user's native capture resolution, so 1.0 is
// tried first and is the common case; the remaining scales let matching survive
// when the source is captured at a different resolution. A single global scale
// (the one that best fits the mission label) is then reused for every other
// template so the glyphs stay crisply aligned.
const SCALES: [f64; 10] = [1.0, 0.9, 1.1, 0.8, 1.2, 0.75, 1.33, 0.67, 1.5, 0.6];

#[derive(Clone, Copy)]
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

struct LevelMatch {
    mission: i32,
    part: i32,
    difficulty: i32,
    times: Vec<i32>,
}

// Lightweight phase timer. When the GE_CV_TIMING environment variable is set,
// each lap() logs the milliseconds elapsed since the previous lap to stderr.
struct PhaseTimer {
    start: Instant,
    last: Instant,
    enabled: bool,
}

impl PhaseTimer {
    fn new() -> Self {
        let now = Instant::now();
        PhaseTimer { start: now, last: now, enabled: env::var_os("GE_CV_TIMING").is_some() }
    }

    fn lap(&mut self, label: &str) {
        let now = Instant::now();
        if self.enabled {
            let ms = now.duration_since(self.last).as_secs_f64() * 1000.0;
            let total = now.duration_since(self.start).as_secs_f64() * 1000.0;
            eprintln!("[ge_cv timing] {label:<22} {ms:8.2} ms  (total {total:8.2} ms)");
        }
        self.last = now;
    }
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

// Matches the GoldenEye level-stats overlay in a single BGRA frame against the
// template PNGs in `templates_dir`. Mirrors ge_cv_match_level().
fn match_level(bgra_frame: &Mat, lang: &str, templates_dir: &str) -> Result<LevelMatch> {
    let mut result = LevelMatch { mission: -1, part: -1, difficulty: -1, times: Vec::new() };

    // Load the label templates.
    let mut parts = Vec::new();
    for i in 1..=5 {
        parts.push(load_template(templates_dir, lang, &format!("part{i}"))?);
    }
    let mut diffs = Vec::new();
    for i in 1..=4 {
        diffs.push(load_template(templates_dir, lang, &format!("diff{i}"))?);
    }

    // Load base glyph templates once; mission and time matching both scale from
    // these in-memory mats.
    let colon_base = load_template(templates_dir, lang, "colon")?;
    let mut digit_base = Vec::new();
    for v in 0..=9 {
        digit_base.push(load_template(templates_dir, lang, &format!("digit{v}"))?);
    }

    let mut timer = PhaseTimer::new();

    // Convert the BGRA frame to grayscale once; every template is matched
    // against this single-channel frame.
    let mut frame = Mat::default();
    imgproc::cvt_color_def(bgra_frame, &mut frame, imgproc::COLOR_BGRA2GRAY)?;
    timer.lap("grayscale");

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
        let colon_tmpl = scaled(&colon_base, scale)?;
        let mut digit_tmpls = Vec::with_capacity(10);
        for v in 0..=9 {
            digit_tmpls.push(scaled(&digit_base[v], scale)?);
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
    result.part = best_label(&label_region, &parts, global_scale, LABEL_THRESHOLD)?;
    timer.lap("part labels");
    result.difficulty = best_label(&label_region, &diffs, global_scale, LABEL_THRESHOLD)?;
    timer.lap("difficulty labels");

    // Locate the digit and colon glyphs at the same scale.
    let colon_tmpl = scaled(&colon_base, global_scale)?;
    let mut digit_tmpls = Vec::with_capacity(10);
    let mut digit_width_sum = 0;
    for v in 0..=9 {
        let t = scaled(&digit_base[v], global_scale)?;
        digit_width_sum += t.cols();
        digit_tmpls.push(t);
    }
    timer.lap("load glyph templates");

    if colon_tmpl.empty() || digit_width_sum == 0 {
        return Ok(result); // no glyph templates: labels only
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
    collect_detections(&colon_region, &colon_tmpl, GLYPH_THRESHOLD, 0, &mut colons)?;
    // Offset back into frame coordinates.
    for c in &mut colons {
        c.x += colon_x0;
        c.y += colon_y0;
    }
    let colons = suppress(colons, colon_w, colon_h, 0.5);
    timer.lap("colon detection");

    // A valid time is two digits on each side of a colon, on the colon's line,
    // so the only digits that matter live in a narrow band around each colon.
    let band_pad_x = digit_w * 3; // room for two digits plus gaps each side
    let band_pad_y = digit_h; // slack for digit/colon height mismatch
    let mut digits = Vec::new();
    for colon in &colons {
        let x0 = (colon.x - band_pad_x).max(0);
        let y0 = (colon.y - band_pad_y).max(0);
        let x1 = (colon.x + colon_w + band_pad_x).min(frame.cols());
        let y1 = (colon.y + colon_h + band_pad_y).min(frame.rows());
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
    // Suppress across all digit values so overlapping matches of different
    // digits collapse to the single strongest reading.
    let digits = suppress(digits, digit_w, digit_h, 0.5);
    timer.lap("digit detection");

    // Assemble "mm:ss" readings: a valid time is two digits immediately to the
    // left of a colon and two immediately to its right, all on the same line.
    let mut times: Vec<FoundTime> = Vec::new();
    for colon in &colons {
        let colon_center_y = colon.y as f64 + colon_h as f64 / 2.0;

        let mut right: Vec<Detection> = Vec::new();
        let mut left: Vec<Detection> = Vec::new();
        for d in &digits {
            if ((d.y as f64 + digit_h as f64 / 2.0) - colon_center_y).abs() >= digit_h as f64 * 0.35 {
                continue; // not on the colon's text line
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
        times.push(FoundTime { y: colon.y, x: colon.x, seconds: minutes * 60 + seconds });
    }

    // Order top-to-bottom (bucketed by line) then left-to-right.
    let line_bucket = digit_h as f64 * 0.5;
    times.sort_by(|a, b| {
        let ra = (a.y as f64 / line_bucket).round() as i32;
        let rb = (b.y as f64 / line_bucket).round() as i32;
        if ra != rb { ra.cmp(&rb) } else { a.x.cmp(&b.x) }
    });

    result.times = times.into_iter().map(|t| t.seconds).collect();
    timer.lap("time assembly");

    Ok(result)
}

fn run() -> Result<i32> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: {} path/to/png [lang] [templates_dir]", args[0]);
        return Ok(2);
    }

    let image_path = &args[1];
    let lang = args.get(2).map(|s| s.as_str()).unwrap_or("en");
    // Default to the cv_templates/ dir that ships alongside obs2/, resolved
    // relative to this crate at compile time (mirrors GE_TEMPLATES_DIR).
    let default_templates = concat!(env!("CARGO_MANIFEST_DIR"), "/../cv_templates");
    let templates_dir = args.get(3).map(|s| s.as_str()).unwrap_or(default_templates);

    // Benchmarking hook: GE_CV_THREADS caps OpenCV's internal thread pool.
    if let Ok(t) = env::var("GE_CV_THREADS") {
        if let Ok(n) = t.parse::<i32>() {
            core::set_num_threads(n)?;
            eprintln!("[test_match] cv::setNumThreads({n})");
        }
    }

    // Load as BGR, then add an opaque alpha channel so the buffer matches the
    // BGRA frames the matcher expects from OBS.
    let bgr = imgcodecs::imread(image_path, imgcodecs::IMREAD_COLOR)?;
    if bgr.empty() {
        eprintln!("error: could not read image '{image_path}'");
        return Ok(1);
    }

    let mut bgra = Mat::default();
    imgproc::cvt_color_def(&bgr, &mut bgra, imgproc::COLOR_BGR2BGRA)?;

    let result = match_level(&bgra, lang, templates_dir)?;

    println!("opencv:     {}", core::get_version_string()?);
    println!("image:      {} ({}x{})", image_path, bgra.cols(), bgra.rows());
    println!("lang:       {lang}");
    println!("templates:  {templates_dir}");
    println!("mission:    {}", result.mission);
    println!("part:       {}", result.part);
    println!("difficulty: {}", result.difficulty);

    println!("times:      {}", result.times.len());
    for (i, &seconds) in result.times.iter().enumerate() {
        println!("  [{i}] {seconds} ({}:{:02})", seconds / 60, seconds % 60);
    }

    Ok(0)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
