use std::io::Cursor;

/// Copies a `width * height` BGRA slice into a BMP-encoded byte vector.
pub(crate) fn encode_bmp_bgra(pixels: &[u8], width: u32, height: u32) -> std::io::Result<Vec<u8>> {
    let mut image = bmp::Image::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            // Source is BGRA; drop the alpha channel.
            image.set_pixel(x, y, bmp::Pixel::new(pixels[i + 2], pixels[i + 1], pixels[i]));
        }
    }

    let mut out = Cursor::new(Vec::new());
    image.to_writer(&mut out)?;
    Ok(out.into_inner())
}

#[cfg(test)]
#[path = "screenshot_test.rs"]
mod screenshot_test;
