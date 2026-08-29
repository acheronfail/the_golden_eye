use std::env;
use std::hint::black_box;
use std::process::ExitCode;
use std::time::Instant;

use ge_cv::{CaptureRegion, CvMatcher, LevelMatch, WORK_HEIGHT, detect_black_frame, detect_watch};
use opencv::core::{self, Mat, Rect, Size};
use opencv::prelude::*;
use opencv::{Result, imgcodecs, imgproc};
use serde_json::json;

const BLACK_FRAME_BENCH_ITERATIONS: usize = 250;

fn env_truthy(key: &str) -> bool {
    env::var(key)
        .ok()
        .is_some_and(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}

fn configure_runtime() {
    ge_cv::configure(ge_cv::RuntimeConfig {
        debug: env::var_os("GE_CV_DEBUG").is_some(),
        timing: env::var_os("GE_CV_TIMING").is_some(),
        threads_overridden: env::var_os("GE_CV_THREADS").is_some(),
    });
}

fn load_bgra(path: &str) -> Result<Mat> {
    let bgr = imgcodecs::imread(path, imgcodecs::IMREAD_COLOR)?;
    if bgr.empty() {
        return Err(opencv::Error::new(core::StsError, format!("could not read image '{path}'")));
    }

    let mut bgra = Mat::default();
    imgproc::cvt_color_def(&bgr, &mut bgra, imgproc::COLOR_BGR2BGRA)?;
    Ok(bgra)
}

fn obs_capture_emulated_frame(source: &Mat, region: Option<CaptureRegion>) -> Result<Mat> {
    if let Some(region) = region {
        let cx = region.crop_x.clamp(0.0, 1.0);
        let cy = region.crop_y.clamp(0.0, 1.0);
        let mut cw = if region.crop_w <= 0.0 { 1.0 } else { region.crop_w };
        let mut ch = if region.crop_h <= 0.0 { 1.0 } else { region.crop_h };
        if cx + cw > 1.0 {
            cw = 1.0 - cx;
        }
        if cy + ch > 1.0 {
            ch = 1.0 - cy;
        }

        let x = ((source.cols() as f32) * cx).round() as i32;
        let y = ((source.rows() as f32) * cy).round() as i32;
        let w = (((source.cols() as f32) * cw).round() as i32).min(source.cols() - x).max(1);
        let h = (((source.rows() as f32) * ch).round() as i32).min(source.rows() - y).max(1);
        let out_height = WORK_HEIGHT;
        let out_width = ((out_height as f32 * region.out_aspect).round() as i32).max(1);
        let roi = source.roi(Rect::new(x, y, w, h))?;
        let mut out = Mat::default();
        imgproc::resize(&roi, &mut out, Size::new(out_width, out_height), 0.0, 0.0, imgproc::INTER_AREA)?;
        return Ok(out);
    }

    if source.rows() > WORK_HEIGHT {
        let scale = WORK_HEIGHT as f64 / source.rows() as f64;
        let width = ((source.cols() as f64 * scale).round() as i32).max(1);
        let mut out = Mat::default();
        imgproc::resize(source, &mut out, Size::new(width, WORK_HEIGHT), 0.0, 0.0, imgproc::INTER_AREA)?;
        Ok(out)
    } else {
        source.try_clone()
    }
}

fn result_json(result: &ge_cv::LevelMatch) -> serde_json::Value {
    json!({
        "screen": result.screen.as_str(),
        "mission": result.mission,
        "part": result.part,
        "difficulty": result.difficulty,
        "detected_lang": result.detected_lang,
        "times": result.times,
        "raw_times": result.raw_times,
        "match_regions": result.match_regions,
        "annotation_sets": result.annotation_sets,
        "runtime_ms": result.runtime_ms,
    })
}

fn benchmark_monitor_cv_sample(matcher: &CvMatcher, frame: &Mat) -> Result<(LevelMatch, serde_json::Value)> {
    let width = frame.cols() as u32;
    let height = frame.rows() as u32;
    let bytes = frame.data_bytes()?;
    let pipeline_started = Instant::now();
    let result = matcher.match_level_from_bgra_frame(frame)?;
    let active_picture = matcher.active_picture_region(width, height);
    let black_frame = detect_black_frame(bytes, width, height, active_picture)
        .ok_or_else(|| opencv::Error::new(core::StsError, "could not sample benchmark frame".to_owned()))?;
    let monitor_cv_runtime_ms = pipeline_started.elapsed().as_secs_f64() * 1000.0;

    let black_frame_started = Instant::now();
    for _ in 0..BLACK_FRAME_BENCH_ITERATIONS {
        let active_picture = black_box(matcher.active_picture_region(width, height));
        black_box(detect_black_frame(black_box(bytes), width, height, active_picture));
    }
    let black_frame_runtime_us =
        black_frame_started.elapsed().as_secs_f64() * 1_000_000.0 / BLACK_FRAME_BENCH_ITERATIONS as f64;

    let mut payload = result_json(&result);
    let object = payload.as_object_mut().expect("result payload is an object");
    object.insert("black_frame_runtime_us".to_owned(), json!(black_frame_runtime_us));
    object.insert("monitor_cv_runtime_ms".to_owned(), json!(monitor_cv_runtime_ms));
    object.insert("black_frame".to_owned(), json!(black_frame));
    Ok((result, payload))
}

fn run() -> Result<i32> {
    configure_runtime();
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: {} <lang> path/to/image.{{png,bmp,...}} [templates_dir]", args[0]);
        return Ok(2);
    }

    let lang = args[1].as_str();
    let image_path = &args[2];
    // Default to the cv_templates/ dir that ships alongside obs2/, resolved
    // relative to this crate at compile time (mirrors GE_TEMPLATES_DIR).
    let default_templates = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cv_templates");
    let templates_dir = args.get(3).map(|s| s.as_str()).unwrap_or(default_templates);
    let diagnostics = env_truthy("GE_CV_DIAGNOSTICS");

    // Benchmarking hook: GE_CV_THREADS caps OpenCV's internal thread pool.
    if let Some(n) = env::var("GE_CV_THREADS").ok().and_then(|value| value.parse().ok()) {
        core::set_num_threads(n)?;
        eprintln!("[test_match] cv::setNumThreads({n})");
    }

    // Load as BGRA so the buffer matches the frames the matcher expects from OBS.
    let bgra = match load_bgra(image_path) {
        Ok(bgra) => bgra,
        Err(err) => {
            eprintln!("error: {err}");
            return Ok(1);
        }
    };

    let source_image = json!({ "path": image_path, "width": bgra.cols(), "height": bgra.rows() });

    let bench_capture_mode = env::var("GE_CV_BENCH_CAPTURE").unwrap_or_else(|_| "fixture".to_owned());
    if !matches!(bench_capture_mode.as_str(), "fixture" | "obs") {
        eprintln!("error: GE_CV_BENCH_CAPTURE must be 'fixture' or 'obs', got {bench_capture_mode:?}");
        return Ok(1);
    }

    // GE_CV_BENCH=N reuses a single matcher across N matches (as the OBS monitor
    // loop does), printing each runtime to stderr so the scale-cache speedup
    // from the first frame to the rest is visible.
    if let Some(runs) = env::var("GE_CV_BENCH").ok().map(|value| value.parse().unwrap_or(5)) {
        let target_warmups = env::var("GE_CV_BENCH_WARMUPS").ok().and_then(|value| value.parse().ok()).unwrap_or(0);
        let json_output = env_truthy("GE_CV_BENCH_JSON");
        let matcher = CvMatcher::new(lang, templates_dir)?;
        let mut cache_warm = Vec::new();
        let mut capture_region = None;

        // GE_CV_BENCH_WARM=path primes the scale cache with one overlay frame
        // first, so the benched frame is matched as it would be mid-session.
        if let Ok(warm) = env::var("GE_CV_BENCH_WARM") {
            let wbgra = load_bgra(&warm)?;
            let warm_frame =
                if bench_capture_mode == "obs" { obs_capture_emulated_frame(&wbgra, None)? } else { wbgra };
            let (r, sample) = benchmark_monitor_cv_sample(&matcher, &warm_frame)?;
            cache_warm.push(sample);
            capture_region = matcher.capture_region();
            if !json_output {
                eprintln!("[bench] warm: {:.2} ms (mission={} part={})", r.runtime_ms, r.mission, r.part);
            }
        }

        let bench_bgra = if bench_capture_mode == "obs" {
            obs_capture_emulated_frame(&bgra, capture_region)?
        } else {
            bgra.try_clone()?
        };

        let mut warmups = Vec::with_capacity(target_warmups);
        for i in 0..target_warmups {
            let (r, sample) = benchmark_monitor_cv_sample(&matcher, &bench_bgra)?;
            if !json_output {
                eprintln!(
                    "[bench] warmup {i}: {:.2} ms (mission={} part={} diff={})",
                    r.runtime_ms, r.mission, r.part, r.difficulty
                );
            }
            warmups.push(sample);
        }

        let mut samples = Vec::with_capacity(runs);
        for i in 0..runs {
            let (r, sample) = benchmark_monitor_cv_sample(&matcher, &bench_bgra)?;
            if !json_output {
                eprintln!(
                    "[bench] run {i}: {:.2} ms (mission={} part={} diff={})",
                    r.runtime_ms, r.mission, r.part, r.difficulty
                );
            }
            samples.push(sample);
        }

        if json_output {
            println!(
                "{}",
                json!({
                    "opencv": core::get_version_string()?,
                    "image": source_image,
                    "bench_image": { "width": bench_bgra.cols(), "height": bench_bgra.rows() },
                    "bench_capture": {
                        "mode": bench_capture_mode,
                        "work_height": WORK_HEIGHT,
                        "capture_region": capture_region,
                    },
                    "bench_black_frame": {
                        "iterations_per_sample": BLACK_FRAME_BENCH_ITERATIONS,
                    },
                    "lang": lang,
                    "templates_dir": templates_dir,
                    "cache_warm": cache_warm,
                    "warmups": warmups,
                    "samples": samples,
                })
            );
            return Ok(0);
        }
    }

    let matcher = CvMatcher::new(lang, templates_dir)?.with_diagnostics(diagnostics);
    let result = matcher.match_level_from_bgra_frame(&bgra)?;
    let active_picture = matcher.active_picture_region(bgra.cols() as u32, bgra.rows() as u32);
    let black_frame = detect_black_frame(bgra.data_bytes()?, bgra.cols() as u32, bgra.rows() as u32, active_picture);
    let watch = detect_watch(bgra.data_bytes()?, bgra.cols() as u32, bgra.rows() as u32, active_picture);

    println!(
        "{}",
        json!({
            "opencv": core::get_version_string()?,
            "image": source_image,
            "lang": lang,
            "templates_dir": templates_dir,
            "screen": result.screen.as_str(),
            "mission": result.mission,
            "part": result.part,
            "difficulty": result.difficulty,
            "detected_lang": result.detected_lang,
            "times": result.times,
            "raw_times": result.raw_times,
            "match_regions": result.match_regions,
            "annotation_sets": result.annotation_sets,
            "runtime_ms": result.runtime_ms,
            "black_frame": black_frame.is_some_and(|signal| signal.detected),
            "watch": watch,
        })
    );

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
