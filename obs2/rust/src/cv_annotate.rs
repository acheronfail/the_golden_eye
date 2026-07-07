use opencv::core::{self, Mat, Point, Rect, Scalar};
use opencv::prelude::*;
use opencv::{Result, imgcodecs, imgproc};

use crate::cv::LevelMatch;

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

    // Dense clusters, such as the header colons, need nearby rows to keep every
    // label readable without pushing callouts across the whole frame.
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

fn summary(result: &LevelMatch) -> String {
    format!(
        "{} m={} p={} d={} regions={}",
        result.screen.as_str(),
        result.mission,
        result.part,
        result.difficulty,
        result.match_regions.len()
    )
}

pub fn annotate_level_match(image: &mut Mat, result: &LevelMatch) -> Result<()> {
    let header_w = image.cols();
    let header_h = 32.min(image.rows()).max(1);
    let header_rect = Rect::new(0, 0, header_w, header_h);
    let mut label_occupied = vec![header_rect];
    let mut labels = Vec::new();

    for (i, region) in result.match_regions.iter().enumerate() {
        let color = color_for(i);
        let x = region.x.clamp(0, image.cols().saturating_sub(1));
        let y = region.y.clamp(0, image.rows().saturating_sub(1));
        let w = region.w.min(image.cols() - x).max(1);
        let h = region.h.min(image.rows() - y).max(1);
        let target = Rect::new(x, y, w, h);
        imgproc::rectangle(image, target, color, 2, imgproc::LINE_8, 0)?;
        let text = format!("{} {:.2}", region.label, region.score);
        let label = place_label(image, &mut label_occupied, target, &text)?;
        labels.push((label, target, text, color));
    }

    for (label, target, text, color) in labels {
        if !rects_overlap(label, target, 0) {
            imgproc::line(image, rect_center(label), rect_center(target), color, 1, imgproc::LINE_AA, 0)?;
        }
        draw_label(image, label, &text, color)?;
    }

    imgproc::rectangle(image, header_rect, Scalar::new(0.0, 0.0, 0.0, 255.0), imgproc::FILLED, imgproc::LINE_8, 0)?;
    imgproc::put_text(
        image,
        &summary(result),
        Point::new(8, 22),
        imgproc::FONT_HERSHEY_SIMPLEX,
        0.65,
        Scalar::new(255.0, 255.0, 255.0, 255.0),
        1,
        imgproc::LINE_AA,
        false,
    )?;

    Ok(())
}

pub fn annotated_png_from_bgra(frame: &[u8], width: u32, height: u32, result: &LevelMatch) -> Result<Vec<u8>> {
    let bgra = Mat::new_rows_cols_with_bytes::<core::Vec4b>(height as i32, width as i32, frame)?;
    let mut bgr = Mat::default();
    imgproc::cvt_color_def(&bgra, &mut bgr, imgproc::COLOR_BGRA2BGR)?;
    annotate_level_match(&mut bgr, result)?;

    let mut out = core::Vector::<u8>::new();
    let params = core::Vector::<i32>::new();
    imgcodecs::imencode(".png", &bgr, &mut out, &params)?;
    Ok(out.to_vec())
}
