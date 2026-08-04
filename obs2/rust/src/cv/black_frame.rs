use serde::Serialize;

const GRID_COLUMNS: usize = 32;
const GRID_ROWS: usize = 18;
// Capture chains can raise nominal black close to luma 30. Coverage keeps
// detailed dark scenes from passing at this more tolerant ceiling.
const LUMA_MAX: u8 = 32;
const MEAN_LUMA_MAX: u8 = 32;
const DARK_PERCENT_MIN: u8 = 100;

/// Pixel bounds containing the game picture in the current captured frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivePictureRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl ActivePictureRegion {
    pub fn full(width: u32, height: u32) -> Self {
        Self { x: 0, y: 0, width, height }
    }

    fn clamp(self, frame_width: u32, frame_height: u32) -> Option<Self> {
        let x = self.x.min(frame_width);
        let y = self.y.min(frame_height);
        let width = self.width.min(frame_width.saturating_sub(x));
        let height = self.height.min(frame_height.saturating_sub(y));
        (width > 0 && height > 0).then_some(Self { x, y, width, height })
    }
}

/// Fixed-cost evidence used to classify a near-black capture frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlackFrameSignal {
    pub detected: bool,
    pub mean_luma: u8,
    pub dark_pixel_percent: u8,
    pub sample_count: u16,
    pub sample_region: ActivePictureRegion,
}

/// Samples at most 576 evenly distributed active-picture pixels.
pub fn detect_black_frame(
    data: &[u8],
    width: u32,
    height: u32,
    active_picture: ActivePictureRegion,
) -> Option<BlackFrameSignal> {
    let width = usize::try_from(width).ok()?;
    let height = usize::try_from(height).ok()?;
    let expected_len = width.checked_mul(height)?.checked_mul(4)?;
    if width == 0 || height == 0 || data.len() < expected_len {
        return None;
    }

    let active_picture = active_picture.clamp(width as u32, height as u32)?;
    let region_x = active_picture.x as usize;
    let region_y = active_picture.y as usize;
    let region_width = active_picture.width as usize;
    let region_height = active_picture.height as usize;
    let columns = region_width.min(GRID_COLUMNS);
    let rows = region_height.min(GRID_ROWS);
    let mut luma_sum = 0_u32;
    let mut dark_samples = 0_u32;
    for row in 0..rows {
        let y = region_y + ((row * 2 + 1) * region_height / (rows * 2)).min(region_height - 1);
        for column in 0..columns {
            let x = region_x + ((column * 2 + 1) * region_width / (columns * 2)).min(region_width - 1);
            let offset = (y * width + x) * 4;
            let b = u32::from(data[offset]);
            let g = u32::from(data[offset + 1]);
            let r = u32::from(data[offset + 2]);
            let luma = ((29 * b + 150 * g + 77 * r + 128) >> 8) as u8;
            luma_sum += u32::from(luma);
            dark_samples += u32::from(luma <= LUMA_MAX);
        }
    }

    let sample_count = u32::try_from(columns * rows).ok()?;
    let mean_luma = ((luma_sum + sample_count / 2) / sample_count) as u8;
    let dark_pixel_percent = (dark_samples * 100 / sample_count) as u8;
    Some(BlackFrameSignal {
        detected: mean_luma <= MEAN_LUMA_MAX && dark_pixel_percent >= DARK_PERCENT_MIN,
        mean_luma,
        dark_pixel_percent,
        sample_count: sample_count as u16,
        sample_region: active_picture,
    })
}
