use std::env;
use std::process::ExitCode;

// Only `ge_rust::cv` is used below, but linking `ge_rust`'s rlib at all pulls
// in its `#[no_mangle]` FFI entry points (e.g. `ge_rust_start`) unconditionally,
// and this bin must resolve every OBS/bridge symbol they reference even though
// it never calls them. See src/obs_stub.rs for why.
#[path = "../obs_stub.rs"]
mod obs_stub;

use opencv::core::Mat;
use opencv::prelude::*;
use opencv::{Result, imgcodecs, imgproc};
use serde_json::json;

fn run() -> Result<i32> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: {} <lang> input.png [templates_dir]", args[0]);
        return Ok(2);
    }

    let lang = args[1].as_str();
    let input_path = &args[2];
    let default_templates = concat!(env!("CARGO_MANIFEST_DIR"), "/../cv_templates");
    let templates_dir = args.get(3).map(|s| s.as_str()).unwrap_or(default_templates);

    let bgr = imgcodecs::imread(input_path, imgcodecs::IMREAD_COLOR)?;
    if bgr.empty() {
        eprintln!("error: could not read image '{input_path}'");
        return Ok(1);
    }

    let mut bgra = Mat::default();
    imgproc::cvt_color_def(&bgr, &mut bgra, imgproc::COLOR_BGR2BGRA)?;

    let matcher = ge_rust::cv::CvMatcher::new(lang, templates_dir)?.with_diagnostics(true);
    let result = matcher.match_level_from_bgra_frame(&bgra)?;
    println!(
        "{}",
        json!({
            "image": { "path": input_path, "width": bgra.cols(), "height": bgra.rows() },
            "lang": lang,
            "templates_dir": templates_dir,
            "match": &result,
            "annotation_sets": &result.annotation_sets,
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
