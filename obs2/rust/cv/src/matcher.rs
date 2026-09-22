use std::sync::{Arc, Mutex};

use opencv::core::{self, Mat, Rect, Size, ToInputArray};
use opencv::prelude::*;
use opencv::{imgcodecs, imgproc};

use crate::calibration::{
    ActivePicture,
    CaptureRegion,
    FOLDER_STRETCH_ASPECT,
    FrameCalibration,
    LevelGeometry,
    RegionMapper,
    TARGET_ASPECT,
    WORK_HEIGHT,
    detect_folder_aspect,
    fractional_rect,
};
use crate::config::{dbg_cv, runtime_config};
use crate::match_result::{
    AnnotationRect,
    LevelMatch,
    MatchRegion,
    Screen,
    annotation_sets,
    has_overlay_markers,
    reject_untrusted_screen,
};
use crate::overlay_reading::{
    COLON_REGION_H,
    COLON_REGION_W,
    COLON_REGION_X,
    COLON_REGION_Y,
    DigitDiscriminator,
    FoundMission,
    FoundTime,
    GLYPH_THRESHOLD,
    HEADER_REGION_H,
    HEADER_REGION_W,
    HEADER_REGION_X,
    HEADER_REGION_Y,
    HEADER_ROW_OFFSET,
    ScaledGlyphs,
    TIME_GATE_COLON_THRESHOLD,
    TIME_GATE_STRONG_COLON,
    detect_header_colons,
    find_mission_from_colons,
    find_times_band,
    format_seconds,
    header_colon_regions,
};
use crate::parallel::par_map;
use crate::template_matching::{
    MatchRect,
    best_label_match_over_scales,
    best_label_match_with_wider_ties,
    best_label_near_row,
    best_match,
    candidate_scales,
    load_template,
    scaled,
};
use crate::{ActivePictureRegion, PhaseTimer, Result, detect_watch, watch};

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
// Screen classification. Each header screen has a banner word below the header
// stack; the four report screens share a "REPORT:" banner and differ in a status
// value below it. Strongest template match in its band above this threshold wins.
const SCREEN_THRESHOLD: f64 = 0.78;
// (x, y, w, h) as fractions of the frame.
const SCREEN_BANNER_REGION: (f64, f64, f64, f64) = (0.04, 0.39, 0.56, 0.11);
const SCREEN_STATUS_REGION: (f64, f64, f64, f64) = (0.18, 0.47, 0.48, 0.10);
// Language detection uses the side tab on the level-start briefing: short,
// static, and distinct between the English and Japanese ROMs, so it rejects a
// wrong game/template language before a same-shaped banner is misclassified.
const LANGUAGE_START_THRESHOLD: f64 = 0.82;
const LANGUAGE_START_MARGIN: f64 = 0.12;
// The language marker is the vertical START tab on the right of the level-start
// briefing. It is fixed near the top-right of both 4:3 and 16:9 captures, so
// there is no need to search the whole frame.
const LANGUAGE_START_REGION: (f64, f64, f64, f64) = (0.68, 0.035, 0.30, 0.35);
// The mission-select grid carries none of the shared header colons, so the gate
// rejects it. It is instead recognized by its film-strip divider (static, en/jp
// identical); strongest match above this threshold classifies it as `Levels`.
const LEVELS_THRESHOLD: f64 = 0.70;
// (x, y, w, h) as fractions of the frame: a band over the left half of the film
// strip spanning the first two inter-row dividers. Two give redundancy (the
// crosshair can cover one); a tight band keeps the match cheap.
const LEVELS_REGION: (f64, f64, f64, f64) = (0.04, 0.20, 0.52, 0.42);
// Match the header gate's colon threshold: GPU-downscaled HDMI captures can
// fall below 0.86. The adjacent digit must still clear its own glyph threshold.
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
    calibration_cache: Mutex<Option<FrameCalibration>>,
    // Lazily populated because cold scale recovery may try several scales, but
    // a live source normally settles on one work scale and one native scale.
    glyph_cache: Mutex<Vec<(u64, Arc<ScaledGlyphs>)>>,
}

impl CvMatcher {
    pub fn new(lang: &str, templates_dir: &str) -> Result<Self> {
        // Pin OpenCV's parallel backend to one thread: we drive parallelism with
        // `par_map`, so a multi-threaded backend would oversubscribe cores and
        // spike tail latency. `GE_CV_THREADS` opts out for benchmarking.
        if !runtime_config().threads_overridden {
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
            calibration_cache: Mutex::new(None),
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

    fn folder_annotation(&self, calib: FrameCalibration) -> Option<AnnotationRect> {
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

    // Learns active-picture bounds and any matcher-only geometry correction.
    // Folderless frames use a full-frame, uncorrected fallback.
    fn calibrate_frame(&self, bgra_frame: &impl ToInputArray, gray: &Mat) -> Result<(Mat, FrameCalibration)> {
        let (w, h) = (gray.cols(), gray.rows());

        if let Some(c) = self.calibration_cache.lock().ok().and_then(|c| *c).filter(|c| c.src_w == w && c.src_h == h) {
            return Ok((c.apply(gray)?, c));
        }

        let Some(folder) = detect_folder_aspect(bgra_frame, w, h)? else {
            let calib = FrameCalibration::uncalibrated(w, h);
            return Ok((gray.try_clone()?, calib));
        };

        let active_picture = ActivePicture::detect(gray)?;
        let level_geometry = if folder.aspect > FOLDER_STRETCH_ASPECT {
            let target_w = (((h as f64) * TARGET_ASPECT).round() as i32).max(1);
            dbg_cv!(
                "[calibrate] {w}x{h} folder_aspect={:.3} active={:?} stretched -> crop {}+{} squish to {target_w}",
                folder.aspect,
                active_picture.rect,
                active_picture.rect.x,
                active_picture.rect.width
            );
            LevelGeometry { crop_x: active_picture.rect.x, crop_w: active_picture.rect.width, target_w }
        } else {
            dbg_cv!(
                "[calibrate] {w}x{h} folder_aspect={:.3} active={:?} matcher geometry unchanged",
                folder.aspect,
                active_picture.rect
            );
            LevelGeometry::identity(w)
        };
        let calib =
            FrameCalibration { src_w: w, src_h: h, active_picture, level_geometry, folder_rect: Some(folder.rect) };

        if let Ok(mut cache) = self.calibration_cache.lock() {
            *cache = Some(calib);
        }
        Ok((calib.apply(gray)?, calib))
    }

    /// The capture transform learned for the current source, or `None` while
    /// uncalibrated or already 4:3. The monitor feeds it back so the GPU
    /// crops+un-stretches frames directly; stable once non-`None`.
    pub fn capture_region(&self) -> Option<CaptureRegion> {
        let calib = (*self.calibration_cache.lock().ok()?)?;
        if calib.is_identity() {
            return None;
        }
        Some(calib.capture_region())
    }

    /// Active game-picture bounds for this exact capture size. A size change
    /// falls back to the full frame until that new shape is calibrated.
    pub fn active_picture_region(&self, width: u32, height: u32) -> ActivePictureRegion {
        let Ok(width_i32) = i32::try_from(width) else {
            return ActivePictureRegion::full(width, height);
        };
        let Ok(height_i32) = i32::try_from(height) else {
            return ActivePictureRegion::full(width, height);
        };
        self.calibration_cache
            .lock()
            .ok()
            .and_then(|cache| *cache)
            .filter(|calib| calib.src_w == width_i32 && calib.src_h == height_i32)
            .map_or_else(|| ActivePictureRegion::full(width, height), |calib| calib.active_picture.region())
    }

    // Reads the mission number inside `rect` of native-res `gray`, sweeping
    // `scales` (implied first) and stopping at the first confident digit. Returns
    // the match and its scale; result coordinates are relative to `rect`.
    fn read_mission(&self, gray: &Mat, rect: Rect, scales: &[f64]) -> Result<(FoundMission, f64)> {
        let region = gray.roi(rect)?;
        let mut found = FoundMission {
            mission: -1,
            score: GLYPH_THRESHOLD,
            fixed_slot: false,
            colon_cx: -1,
            colon_cy: -1,
            colon: None,
            digit: None,
        };
        let mut scale_used = scales.first().copied().unwrap_or(1.0);
        // Sweep scales sequentially to preserve the early-exit: a native-res
        // mission read is expensive, so the implied scale (tried first) must
        // short-circuit. Parallelism lives inside `find_mission_from_colons`.
        for &scale in scales {
            let glyphs = self.scaled_glyphs(scale)?;
            let f = find_mission_from_colons(&region, &glyphs)?;
            dbg_cv!(
                "[mission] scale={scale:.3} m={} score={:.3} cx={} cy={}",
                f.mission,
                f.score,
                f.colon_cx,
                f.colon_cy
            );
            // Fixed-slot confidence uses a different metric from free template matches.
            // Once accepted, preserve its digit and row anchor across scales.
            if f.fixed_slot {
                return Ok((f, scale));
            }
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
        let watch_detection = if self.diagnostics {
            bgra_frame
                .input_array()
                .ok()
                .and_then(|input| input.get_mat_def().ok())
                .and_then(|frame| {
                    let width = frame.cols() as u32;
                    let height = frame.rows() as u32;
                    let active_picture = self.active_picture_region(width, height);
                    detect_watch(frame.data_bytes().ok()?, width, height, active_picture)
                })
                .map(watch::annotation_set)
        } else {
            None
        };

        // Convert the BGRA frame to grayscale once; every template is matched
        // against this single-channel frame.
        let mut gray = Mat::default();
        imgproc::cvt_color_def(bgra_frame, &mut gray, imgproc::COLOR_BGRA2GRAY)?;

        // Restore a 4:3 picture that an HDMI converter stretched wide, so glyphs
        // regain the proportions templates expect. Calibrated once per resolution
        // off the folder; a no-op on clean 4:3 or pillarboxed grabs.
        let (gray, calib) = self.calibrate_frame(bgra_frame, &gray)?;
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
            result.annotation_sets =
                annotation_sets(watch_detection, &result.match_regions, search_regions, folder_region, Vec::new());
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
                result.annotation_sets =
                    annotation_sets(watch_detection, &result.match_regions, search_regions, folder_region, Vec::new());
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
            let part = best_label_near_row(
                &label_region,
                &self.parts,
                global_scale,
                LABEL_THRESHOLD,
                mission_cy + (colon_h as f64 * HEADER_ROW_OFFSET).round() as i32,
                colon_h,
                false,
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
            let difficulty = best_label_near_row(
                &label_region,
                &self.diffs,
                global_scale,
                LABEL_THRESHOLD,
                mission_cy - (colon_h as f64 * HEADER_ROW_OFFSET).round() as i32,
                colon_h,
                true,
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
            if let Some((difficulty, r)) =
                best_label_match_with_wider_ties(&label_region, &self.diffs, global_scale, LABEL_THRESHOLD, true)?
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
                AnnotationRect {
                    label: label.clone(),
                    x: region.x,
                    y: region.y,
                    w: region.w,
                    h: region.h,
                    score: Some(region.score),
                }
            })
            .collect();
        let times: Vec<i32> = found_times.into_iter().map(|t| t.seconds).collect();
        result.times = ge_game::Times::classify(result.mission, result.part, result.difficulty, &times);
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
        result.annotation_sets =
            annotation_sets(watch_detection, &result.match_regions, search_regions, folder_region, time_digits);

        Ok(result)
    }
}
