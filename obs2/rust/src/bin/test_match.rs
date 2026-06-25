use std::env;
use std::process::ExitCode;

use opencv::core::{self, Mat};
use opencv::prelude::*;
use opencv::{Result, imgcodecs, imgproc};
use serde_json::json;

fn run() -> Result<i32> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: {} path/to/png [templates_dir]", args[0]);
        return Ok(2);
    }

    let image_path = &args[1];
    let ge_lang = env::var("GE_LANG").unwrap_or_else(|_| "en".to_string());
    let lang = ge_lang.as_str();
    // Default to the cv_templates/ dir that ships alongside obs2/, resolved
    // relative to this crate at compile time (mirrors GE_TEMPLATES_DIR).
    let default_templates = concat!(env!("CARGO_MANIFEST_DIR"), "/../cv_templates");
    let templates_dir = args.get(2).map(|s| s.as_str()).unwrap_or(default_templates);

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

    let result = ge_rust::cv::match_level(&bgra, lang, templates_dir)?;

    println!(
        "{}",
        json!({
            "opencv": core::get_version_string()?,
            "image": { "path": image_path, "width": bgra.cols(), "height": bgra.rows() },
            "lang": lang,
            "templates_dir": templates_dir,
            "mission": result.mission,
            "part": result.part,
            "difficulty": result.difficulty,
            "times": result.times,
            "runtime_ms": result.runtime_ms,
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
