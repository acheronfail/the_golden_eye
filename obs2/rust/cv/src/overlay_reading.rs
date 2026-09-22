use opencv::core::{self, Mat, Rect, Size, ToInputArray};
use opencv::imgproc;
use opencv::prelude::*;

use crate::Result;
use crate::calibration::clamp_rect;
use crate::config::dbg_cv;
use crate::parallel::par_map;
use crate::template_matching::{Detection, MatchRect, collect_detections, scaled, suppress};

// Region searched by the entry gate for stats-overlay header colons. Both the
// level-start and stats screens carry the same three left-aligned header rows
// ending in colons; counting strong colons admits them but rejects gameplay.
pub(super) const HEADER_REGION_X: f64 = 0.08;
pub(super) const HEADER_REGION_W: f64 = 0.56;
pub(super) const HEADER_REGION_Y: f64 = 0.18;
pub(super) const HEADER_REGION_H: f64 = 0.30;

const COLON_ANCHOR_THRESHOLD: f64 = 0.84;
// The entry gate admits a frame only with two header colons AND at least one
// confident match. Thresholds sit low (0.8s / 0.85) to admit blurry composite/
// HDMI grabs yet reject gameplay; any non-stats frame that slips in reads no times.
pub(super) const TIME_GATE_COLON_THRESHOLD: f64 = 0.84;
pub(super) const TIME_GATE_STRONG_COLON: f64 = 0.85;

// Header rows are evenly spaced by just under one glyph height. Once the mission
// colon is known, keep part/difficulty matching close to their fixed row centres.
pub(super) const HEADER_ROW_OFFSET: f64 = 0.88;
// Region searched for the time colons (upper stats table). Kept generous to
// tolerate overlay drift from letterboxing/rescaling; downstream "mm:ss" spacing
// checks reject label colons. Bottom ~0.62 catches Time/Best but not lower rows.
pub(super) const COLON_REGION_X: f64 = 0.15;
pub(super) const COLON_REGION_W: f64 = 0.62;
pub(super) const COLON_REGION_Y: f64 = 0.45;
pub(super) const COLON_REGION_H: f64 = 0.17;
// A mission digit read in the colon-anchored fixed slot must clear this weighted
// discriminator score before it replaces the free-moving template detection.
const MISSION_FIXED_ACCEPT: f64 = 0.82;
// Correlation needed to accept an individual digit/colon glyph.
pub(super) const GLYPH_THRESHOLD: f64 = 0.78;
// A time recovered from the screen, kept with its position so the final array
// can be ordered top-to-bottom then left-to-right.
pub(super) struct FoundTime {
    pub(super) y: i32,
    pub(super) x: i32,
    pub(super) seconds: i32,
    pub(super) colon: MatchRect,
}

pub(super) fn format_seconds(seconds: i32) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

#[derive(Clone, Copy)]
pub(super) struct FoundMission {
    pub(super) mission: i32,
    pub(super) score: f64,
    // Centre of the anchoring "Mission N:" colon, in region coordinates. The
    // vertical centre pins the difficulty (up) and part (down) rows; both let a
    // later frame re-search the mission in a tight box instead of the header.
    pub(super) colon_cx: i32,
    pub(super) colon_cy: i32,
    pub(super) colon: Option<MatchRect>,
    pub(super) digit: Option<MatchRect>,
}

// What the entry gate found in the header colon band: how many colons, the peak
// correlation, and the scale they matched best at. The scale is reused for the
// label searches so they are not re-run across every candidate scale.
pub(super) struct HeaderColons {
    pub(super) count: usize,
    pub(super) peak: f64,
    pub(super) scale: f64,
}

// Detects header colons inside `region` (fractional x/y/w/h), trying every
// candidate scale so the gate works at any capture resolution. Returns the
// richest result (most colons, then peak) and its scale, stopping early.
pub(super) fn detect_header_colons(
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

pub(super) fn header_colon_regions(
    frame: &Mat,
    base_colon: &Mat,
    scale: f64,
    threshold: f64,
) -> Result<Vec<MatchRect>> {
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
pub(super) fn find_mission_from_colons(
    label_region: &(impl MatTraitConst + ToInputArray),
    glyphs: &ScaledGlyphs,
) -> Result<FoundMission> {
    let colon_tmpl = &glyphs.colon;
    let digit_tmpls = &glyphs.digits;
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

    // A cold header has three fixed-spaced rows; choose the middle colon before
    // reading its digit. A cached ROI has only the mission row, so its strongest
    // colon is already the correct anchor.
    let mut row_groups: Vec<Vec<Detection>> = Vec::new();
    let mut by_y = colons.clone();
    by_y.sort_by_key(|c| c.y);
    for colon in by_y {
        let cy = colon.y + colon_h / 2;
        if let Some(group) = row_groups.last_mut()
            && group.last().is_some_and(|last| (cy - (last.y + colon_h / 2)).abs() <= (colon_h as f64 * 0.45) as i32)
        {
            group.push(colon);
        } else {
            row_groups.push(vec![colon]);
        }
    }
    let row_best = |group: &[Detection]| group.iter().copied().max_by(|a, b| a.score.total_cmp(&b.score));
    let mission_colon = if label_region.rows() <= colon_h * 3 {
        row_groups.iter().filter_map(|g| row_best(g)).max_by(|a, b| a.score.total_cmp(&b.score))
    } else {
        let mut best: Option<(f64, Detection)> = None;
        for rows in row_groups.windows(3) {
            let Some(top) = row_best(&rows[0]) else { continue };
            let Some(middle) = row_best(&rows[1]) else { continue };
            let Some(bottom) = row_best(&rows[2]) else { continue };
            let top_gap = (middle.y - top.y) as f64 / colon_h as f64;
            let bottom_gap = (bottom.y - middle.y) as f64 / colon_h as f64;
            if !(0.55..=1.30).contains(&top_gap) || !(0.55..=1.30).contains(&bottom_gap) {
                continue;
            }
            let spacing_penalty = (top_gap - HEADER_ROW_OFFSET).abs() + (bottom_gap - HEADER_ROW_OFFSET).abs();
            let score = top.score + middle.score + bottom.score - spacing_penalty * 0.15;
            if best.is_none_or(|(best_score, _)| score > best_score) {
                best = Some((score, middle));
            }
        }
        best.map(|(_, colon)| colon)
    };

    // Read the digit from its fixed slot immediately left of the selected colon,
    // using the variance-weighted discriminator introduced for stats times.
    if let Some(colon) = mission_colon {
        let digit_y = colon.y + (colon_h - digit_h) / 2;
        let digit_x = colon.x - digit_w;
        let digit_rect = Rect::new(digit_x, digit_y, digit_w, digit_h);
        let (mission, confidence) = classify_box(label_region, glyphs, digit_rect, -1)?;
        dbg_cv!("[mission fixed] value={mission} score={confidence:.3} x={digit_x} y={digit_y}");
        if (1..=9).contains(&mission) && confidence >= MISSION_FIXED_ACCEPT {
            return Ok(FoundMission {
                mission,
                score: confidence,
                colon_cx: colon.x + colon_w / 2,
                colon_cy: colon.y + colon_h / 2,
                colon: Some(MatchRect { x: colon.x, y: colon.y, w: colon.w, h: colon.h, score: colon.score }),
                digit: Some(MatchRect { x: digit_x, y: digit_y, w: digit_w, h: digit_h, score: confidence }),
            });
        }
    }

    let band_pad_x = digit_w * 2;
    let band_pad_y = digit_h;
    let fallback_colons = mission_colon.map_or_else(|| colons.clone(), |colon| vec![colon]);

    // Each (colon, digit) pair is an independent template search -- the bulk of
    // the mission cost at native resolution. Fan the pairs across cores; each
    // returns its best candidate, reduced to the serial "highest digit wins".
    let work: Vec<(usize, usize)> = (0..fallback_colons.len()).flat_map(|c| (1..=9).map(move |v| (c, v))).collect();
    let search_digit = |k: usize| {
        let (ci, v) = work[k];
        let colon = fallback_colons[ci];
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

pub(super) fn find_times_band(
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
    // Group rows by proximity before sorting left-to-right. Absolute y buckets
    // can split one row when two colon detections differ by just one pixel.
    times.sort_by_key(|time| time.y);
    let mut row_start = 0;
    while row_start < times.len() {
        let row_y = times[row_start].y;
        let row_end =
            row_start + times[row_start..].partition_point(|time| (time.y - row_y) as f64 <= digit_h as f64 * 0.5);
        times[row_start..row_end].sort_by_key(|time| time.x);
        row_start = row_end;
    }

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

// Colon and digit templates resized for one exact scale, used every frame by the
// mission and time readers. Caching avoids re-resizing/blurring all eleven; Arc
// lets the hot path borrow a set without holding the cache lock during matching.
pub(super) struct ScaledGlyphs {
    pub(super) colon: Mat,
    pub(super) digits: Vec<Mat>,
    // Discriminative digit reader (see `DigitDiscriminator`), built from `digits`.
    // None when a template is missing so callers fall back to plain matching.
    pub(super) discriminator: Option<DigitDiscriminator>,
}

// Re-classifies an already-located digit by weighting the correlation towards the
// pixels where the ten glyphs actually differ (an `8`'s middle bar, a `6`/`9`
// opening). Whole-glyph correlation drowns those few pixels in the shared outer
// ring, leaving `0/6/8/9` a hair apart; weighting by inter-glyph variance widens
// that margin ~3x, so per-frame capture noise no longer flips the winner.
pub(super) struct DigitDiscriminator {
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
    pub(super) fn build(digits: &[Mat]) -> Result<Option<Self>> {
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
