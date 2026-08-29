use serde::Serialize;

use super::{ActivePictureRegion, AnnotationRect, AnnotationSet};

const RING_INNER: f32 = 0.62;
const RING_OUTER: f32 = 1.04;
const FACE_OUTER: f32 = 0.58;
const WATCH_RADIUS_SCALE: f32 = 0.47;
const BRIGHT_NEUTRAL_LUMA_MIN: u8 = 105;
const BRIGHT_NEUTRAL_CHROMA_MAX: u8 = 35;
const DARK_RING_LUMA_MAX: u8 = 45;
const CLOCK_TICK_PERCENT_MIN: f32 = 1.0;
const MENU_TICK_PERCENT_MAX: f32 = 0.2;
const CLOCK_GREEN_PERCENT_MIN: f32 = 25.0;
const CLOCK_DARK_RING_PERCENT_MIN: f32 = 50.0;
const MENU_DARK_RING_PERCENT_MIN: f32 = 75.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WatchPresentation {
    Absent,
    ClockFace,
    MenuSurface,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchSignal {
    pub presentation: WatchPresentation,
    pub bright_tick_percent: f32,
    pub green_face_percent: f32,
    pub dark_ring_percent: f32,
    pub sample_region: ActivePictureRegion,
}

/// Experimental watch classifier using only neutral dial geometry.
pub fn detect_watch(data: &[u8], width: u32, height: u32, active_picture: ActivePictureRegion) -> Option<WatchSignal> {
    let width = usize::try_from(width).ok()?;
    let height = usize::try_from(height).ok()?;
    let expected_len = width.checked_mul(height)?.checked_mul(4)?;
    if width == 0 || height == 0 || data.len() < expected_len {
        return None;
    }

    let active_picture = active_picture.clamp(width as u32, height as u32)?;
    let center_x = active_picture.x as f32 + active_picture.width as f32 / 2.0;
    let center_y = active_picture.y as f32 + active_picture.height as f32 / 2.0;
    let radius_x = active_picture.width as f32 * WATCH_RADIUS_SCALE;
    let radius_y = active_picture.height as f32 * WATCH_RADIUS_SCALE;
    let mut ring_samples = 0_u32;
    let mut bright_ticks = 0_u32;
    let mut dark_ring = 0_u32;
    let mut face_samples = 0_u32;
    let mut green_face = 0_u32;
    let mut dark_face = 0_u32;

    for y in active_picture.y..active_picture.y + active_picture.height {
        let dy = (y as f32 + 0.5 - center_y) / radius_y;
        for x in active_picture.x..active_picture.x + active_picture.width {
            let dx = (x as f32 + 0.5 - center_x) / radius_x;
            let radius = (dx * dx + dy * dy).sqrt();
            if radius >= RING_OUTER {
                continue;
            }

            let offset = (y as usize * width + x as usize) * 4;
            let b = data[offset];
            let g = data[offset + 1];
            let r = data[offset + 2];
            let max_channel = b.max(g).max(r);
            let min_channel = b.min(g).min(r);
            let luma = ((29 * u32::from(b) + 150 * u32::from(g) + 77 * u32::from(r) + 128) >> 8) as u8;

            if radius > RING_INNER {
                ring_samples += 1;
                bright_ticks += u32::from(
                    luma > BRIGHT_NEUTRAL_LUMA_MIN
                        && max_channel.saturating_sub(min_channel) < BRIGHT_NEUTRAL_CHROMA_MAX,
                );
                dark_ring += u32::from(luma < DARK_RING_LUMA_MAX);
            } else if radius < FACE_OUTER {
                face_samples += 1;
                green_face += u32::from(g > 20 && i16::from(g) > i16::from(r) + 8 && i16::from(g) > i16::from(b) + 5);
                dark_face += u32::from(luma < DARK_RING_LUMA_MAX);
            }
        }
    }

    let bright_tick_percent = percent(bright_ticks, ring_samples)?;
    let green_face_percent = percent(green_face, face_samples)?;
    let dark_ring_percent = percent(dark_ring, ring_samples)?;
    let clock_face = bright_tick_percent >= CLOCK_TICK_PERCENT_MIN
        && green_face_percent >= CLOCK_GREEN_PERCENT_MIN
        && dark_ring_percent >= CLOCK_DARK_RING_PERCENT_MIN;
    let menu_surface = bright_tick_percent < MENU_TICK_PERCENT_MAX && dark_ring_percent >= MENU_DARK_RING_PERCENT_MIN;
    let fully_dark = dark_ring == ring_samples && dark_face == face_samples;
    let presentation = if fully_dark {
        WatchPresentation::Absent
    } else if clock_face {
        WatchPresentation::ClockFace
    } else if menu_surface {
        WatchPresentation::MenuSurface
    } else if green_face_percent >= CLOCK_GREEN_PERCENT_MIN || dark_ring_percent >= CLOCK_DARK_RING_PERCENT_MIN {
        WatchPresentation::Ambiguous
    } else {
        WatchPresentation::Absent
    };

    Some(WatchSignal {
        presentation,
        bright_tick_percent,
        green_face_percent,
        dark_ring_percent,
        sample_region: active_picture,
    })
}

fn percent(count: u32, total: u32) -> Option<f32> {
    (total > 0).then_some(count as f32 * 100.0 / total as f32)
}

pub(super) fn annotation_set(signal: WatchSignal) -> AnnotationSet {
    AnnotationSet {
        id: "watch_detection".to_owned(),
        label: "Watch detection".to_owned(),
        annotations: vec![
            annotation_bounds(
                signal.sample_region,
                RING_OUTER,
                format!(
                    "dial ring {:?}: ticks {:.2}%, dark {:.2}%",
                    signal.presentation, signal.bright_tick_percent, signal.dark_ring_percent
                ),
            ),
            annotation_bounds(
                signal.sample_region,
                FACE_OUTER,
                format!("watch face: green {:.2}%", signal.green_face_percent),
            ),
        ],
    }
}

fn annotation_bounds(region: ActivePictureRegion, radius: f32, label: String) -> AnnotationRect {
    let radius_x = region.width as f32 * WATCH_RADIUS_SCALE * radius;
    let radius_y = region.height as f32 * WATCH_RADIUS_SCALE * radius;
    let center_x = region.x as f32 + region.width as f32 / 2.0;
    let center_y = region.y as f32 + region.height as f32 / 2.0;
    let x = (center_x - radius_x).floor().max(region.x as f32) as i32;
    let y = (center_y - radius_y).floor().max(region.y as f32) as i32;
    let right = (center_x + radius_x).ceil().min((region.x + region.width) as f32) as i32;
    let bottom = (center_y + radius_y).ceil().min((region.y + region.height) as f32) as i32;
    AnnotationRect { label, x, y, w: right - x, h: bottom - y, score: None }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WatchTransition {
    Paused,
    Resumed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchState {
    pub is_paused: bool,
    pub transition: Option<WatchTransition>,
}

#[derive(Debug, Default)]
pub struct WatchDetector {
    is_paused: bool,
    previous_clock_face: bool,
}

impl WatchDetector {
    pub fn observe(&mut self, signal: WatchSignal) -> WatchState {
        let clock_face = signal.presentation == WatchPresentation::ClockFace;
        let menu_surface = signal.presentation == WatchPresentation::MenuSurface;
        let transition = if !self.is_paused && self.previous_clock_face && menu_surface {
            self.is_paused = true;
            Some(WatchTransition::Paused)
        } else if self.is_paused && clock_face {
            self.is_paused = false;
            Some(WatchTransition::Resumed)
        } else {
            None
        };
        self.previous_clock_face = clock_face;
        WatchState { is_paused: self.is_paused, transition }
    }
}
