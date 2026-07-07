use std::env;
use std::process::ExitCode;

use opencv::core::{self, Mat, Point, Rect, Scalar};
use opencv::prelude::*;
use opencv::{Result, imgcodecs, imgproc};

const LABEL_FONT: i32 = imgproc::FONT_HERSHEY_SIMPLEX;
const LABEL_SCALE: f64 = 0.55;
const LABEL_THICKNESS: i32 = 1;
const LABEL_PAD: i32 = 4;

fn color_for(i: usize) -> Scalar {
    const COLORS: [(f64, f64, f64); 8] = [
        (64.0, 220.0, 255.0),
        (80.0, 255.0, 120.0),
        (255.0, 160.0, 64.0),
        (255.0, 96.0, 220.0),
        (160.0, 128.0, 255.0),
        (64.0, 180.0, 255.0),
        (255.0, 255.0, 96.0),
        (96.0, 255.0, 255.0),
    ];
    let (b, g, r) = COLORS[i % COLORS.len()];
    Scalar::new(b, g, r, 255.0)
}

fn rects_overlap(a: Rect, b: Rect, padding: i32) -> bool {
    let ax0 = a.x - padding;
    let ay0 = a.y - padding;
    let ax1 = a.x + a.width + padding;
    let ay1 = a.y + a.height + padding;
    let bx0 = b.x;
    let by0 = b.y;
    let bx1 = b.x + b.width;
    let by1 = b.y + b.height;
    ax0 < bx1 && ax1 > bx0 && ay0 < by1 && ay1 > by0
}

fn label_rect(image: &Mat, x: i32, y: i32, text: &str) -> Result<Rect> {
    let mut baseline = 0;
    let text_size = imgproc::get_text_size(text, LABEL_FONT, LABEL_SCALE, LABEL_THICKNESS, &mut baseline)?;
    let bg_w = (text_size.width + LABEL_PAD * 2).min(image.cols()).max(1);
    let bg_h = (text_size.height + baseline + LABEL_PAD * 2).min(image.rows()).max(1);
    let label_x = x.clamp(0, (image.cols() - bg_w).max(0));
    let label_y = y.clamp(0, (image.rows() - bg_h).max(0));
    Ok(Rect::new(label_x, label_y, bg_w, bg_h))
}

fn label_candidates(image: &Mat, target: Rect, text: &str) -> Result<Vec<Rect>> {
    let above_y = target.y - 30;
    let below_y = target.y + target.height + 6;
    let right_x = target.x + target.width + 8;
    let left_x = target.x - 260;
    let mut out = vec![
        label_rect(image, target.x, above_y, text)?,
        label_rect(image, target.x + target.width + 8, above_y, text)?,
        label_rect(image, target.x, below_y, text)?,
        label_rect(image, target.x + target.width + 8, below_y, text)?,
        label_rect(image, right_x, target.y, text)?,
        label_rect(image, left_x, target.y, text)?,
    ];

    // Dense clusters, such as the header colons, need a few nearby rows to keep
    // every label readable without pushing callouts across the whole frame.
    for dy in [-96, -72, -48, 48, 72, 96, 120, -120] {
        out.push(label_rect(image, target.x, target.y + dy, text)?);
        out.push(label_rect(image, target.x + target.width + 12, target.y + dy, text)?);
        out.push(label_rect(image, target.x - 220, target.y + dy, text)?);
    }
    Ok(out)
}

fn place_label(image: &Mat, occupied: &mut Vec<Rect>, target: Rect, text: &str) -> Result<Rect> {
    let candidates = label_candidates(image, target, text)?;
    for candidate in candidates {
        if occupied.iter().all(|used| !rects_overlap(candidate, *used, 3)) {
            occupied.push(candidate);
            return Ok(candidate);
        }
    }

    let fallback = label_rect(image, target.x, target.y + target.height + 8, text)?;
    occupied.push(fallback);
    Ok(fallback)
}

fn rect_center(rect: Rect) -> Point {
    Point::new(rect.x + rect.width / 2, rect.y + rect.height / 2)
}

fn draw_label(image: &mut Mat, rect: Rect, text: &str, color: Scalar) -> Result<()> {
    imgproc::rectangle(image, rect, Scalar::new(0.0, 0.0, 0.0, 255.0), imgproc::FILLED, imgproc::LINE_8, 0)?;
    imgproc::put_text(
        image,
        text,
        Point::new(rect.x + LABEL_PAD, rect.y + rect.height - LABEL_PAD - 3),
        LABEL_FONT,
        LABEL_SCALE,
        color,
        LABEL_THICKNESS,
        imgproc::LINE_AA,
        false,
    )?;
    Ok(())
}

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

    let summary = format!(
        "{} m={} p={} d={} regions={}",
        result.screen.as_str(),
        result.mission,
        result.part,
        result.difficulty,
        result.match_regions.len()
    );
    let header_w = bgr.cols();
    let header_h = 32.min(bgr.rows()).max(1);
    let header_rect = Rect::new(0, 0, header_w, header_h);
    let mut label_occupied = vec![header_rect];
    let mut labels = Vec::new();

    for (i, region) in result.match_regions.iter().enumerate() {
        let color = color_for(i);
        let x = region.x.clamp(0, bgr.cols().saturating_sub(1));
        let y = region.y.clamp(0, bgr.rows().saturating_sub(1));
        let w = region.w.min(bgr.cols() - x).max(1);
        let h = region.h.min(bgr.rows() - y).max(1);
        let target = Rect::new(x, y, w, h);
        imgproc::rectangle(&mut bgr, target, color, 2, imgproc::LINE_8, 0)?;
        let text = format!("{} {:.2}", region.label, region.score);
        let label = place_label(&bgr, &mut label_occupied, target, &text)?;
        labels.push((label, target, text, color));
    }

    for (label, target, text, color) in labels {
        if !rects_overlap(label, target, 0) {
            imgproc::line(&mut bgr, rect_center(label), rect_center(target), color, 1, imgproc::LINE_AA, 0)?;
        }
        draw_label(&mut bgr, label, &text, color)?;
    }

    imgproc::rectangle(&mut bgr, header_rect, Scalar::new(0.0, 0.0, 0.0, 255.0), imgproc::FILLED, imgproc::LINE_8, 0)?;
    imgproc::put_text(
        &mut bgr,
        &summary,
        Point::new(8, 22),
        imgproc::FONT_HERSHEY_SIMPLEX,
        0.65,
        Scalar::new(255.0, 255.0, 255.0, 255.0),
        1,
        imgproc::LINE_AA,
        false,
    )?;

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
