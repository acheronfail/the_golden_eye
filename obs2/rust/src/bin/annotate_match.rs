use std::env;
use std::process::ExitCode;

use opencv::core::{self, Mat};
use opencv::prelude::*;
use opencv::{Result, imgcodecs, imgproc};

fn run() -> Result<i32> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: {} <lang> input.png output.png [templates_dir]", args[0]);
        return Ok(2);
    }
    if !ge_rust::cv::CvMatcher::diagnostics_available() {
        eprintln!("error: match diagnostics are only available in debug builds or builds with the `dev` feature");
        return Ok(2);
    }

    let lang = args[1].as_str();
    let input_path = &args[2];
    let output_path = &args[3];
    let default_templates = concat!(env!("CARGO_MANIFEST_DIR"), "/../cv_templates");
    let templates_dir = args.get(4).map(|s| s.as_str()).unwrap_or(default_templates);

    let mut bgr = imgcodecs::imread(input_path, imgcodecs::IMREAD_COLOR)?;
    if bgr.empty() {
        eprintln!("error: could not read image '{input_path}'");
        return Ok(1);
    }

    let mut bgra = Mat::default();
    imgproc::cvt_color_def(&bgr, &mut bgra, imgproc::COLOR_BGR2BGRA)?;

    let matcher = ge_rust::cv::CvMatcher::new(lang, templates_dir)?.with_diagnostics(true);
    if !matcher.diagnostics_enabled() {
        eprintln!("error: match diagnostics are disabled in this build");
        return Ok(2);
    }
    let result = matcher.match_level_from_bgra_frame(&bgra)?;

    ge_rust::cv_annotate::annotate_level_match(&mut bgr, &result)?;

    let params: core::Vector<i32> = core::Vector::new();
    if !imgcodecs::imwrite(output_path, &bgr, &params)? {
        eprintln!("error: could not write image '{output_path}'");
        return Ok(1);
    }
    eprintln!(
        "wrote {output_path} with {} match region{}",
        result.match_regions.len(),
        if result.match_regions.len() == 1 { "" } else { "s" }
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
