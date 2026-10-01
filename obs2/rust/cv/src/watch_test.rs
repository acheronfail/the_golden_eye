use super::*;

// Original full-frame scan provides an independent oracle for cached spans.
fn reference_watch(data: &[u8], width: u32, height: u32, active_picture: ActivePictureRegion) -> Option<WatchSignal> {
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

#[test]
fn cached_spans_match_full_scan_across_geometry_and_pixel_changes() {
    let mut seed = 0x1234_5678_u32;
    for (width, height) in [(1, 1), (2, 3), (17, 19), (640, 480), (853, 480), (641, 479)] {
        let mut data = vec![0; width as usize * height as usize * 4];
        let regions = [
            ActivePictureRegion::full(width, height),
            ActivePictureRegion { x: width / 7, y: height / 9, width: width * 3 / 4, height: height * 2 / 3 },
            ActivePictureRegion { x: width / 3, y: height / 3, width, height },
            ActivePictureRegion::full(width, height),
        ];
        for region in regions {
            for mode in 0..4 {
                for byte in &mut data {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    *byte = match mode {
                        0 => 0,
                        1 => 255,
                        _ => (seed >> 24) as u8,
                    };
                }
                assert_eq!(
                    detect_watch(&data, width, height, region),
                    reference_watch(&data, width, height, region),
                    "{width}x{height}, {region:?}, mode={mode}"
                );
            }
        }
    }
}

#[test]
fn cached_spans_follow_stride_changes_with_the_same_crop() {
    let region = ActivePictureRegion { x: 3, y: 5, width: 20, height: 14 };
    for width in [31, 32, 31] {
        let data: Vec<u8> = (0..width * 24 * 4).map(|i| (i * 73 + i / 17) as u8).collect();
        assert_eq!(detect_watch(&data, width, 24, region), reference_watch(&data, width, 24, region));
    }
}

#[test]
fn cached_spans_reject_invalid_buffers_and_empty_regions() {
    let data = vec![0; 16 * 16 * 4];
    let region = ActivePictureRegion::full(16, 16);
    assert!(detect_watch(&data, 16, 16, region).is_some());
    assert!(detect_watch(&data[..data.len() - 1], 16, 16, region).is_none());
    assert!(detect_watch(&data, 0, 16, region).is_none());
    assert!(detect_watch(&data, 16, 0, region).is_none());
    assert!(detect_watch(&data, 16, 16, ActivePictureRegion { x: 16, ..region }).is_none());
    assert!(detect_watch(&data, u32::MAX, u32::MAX, region).is_none());
}
