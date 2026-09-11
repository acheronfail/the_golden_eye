use opencv::prelude::*;
use opencv::{core, imgcodecs};

use super::*;

/// The frame dump writes these BMPs and `test_match` reads them back with
/// OpenCV's `imread`; guard that the encoded format stays decodable and keeps
/// its dimensions and BGR pixel values (imread yields BGR from a BGRA source).
#[test]
fn dumped_bmp_decodes_through_imread() {
    // 2x2 BGRA: distinct colours so a channel/row swap would be caught.
    let pixels = [
        10, 20, 30, 255, 40, 50, 60, 255, // row 0
        70, 80, 90, 255, 100, 110, 120, 255, // row 1
    ];
    let bmp = encode_bmp_bgra(&pixels, 2, 2).expect("encode bmp");

    let path = std::env::temp_dir().join(format!("ge-bmp-roundtrip-{}.bmp", std::process::id()));
    std::fs::write(&path, bmp).expect("write bmp");
    let decoded = imgcodecs::imread(path.to_str().unwrap(), imgcodecs::IMREAD_COLOR).expect("imread");
    std::fs::remove_file(&path).ok();

    assert!(!decoded.empty(), "imread could not decode the dumped BMP");
    assert_eq!((decoded.cols(), decoded.rows()), (2, 2));
    // Top-left pixel round-trips its BGR bytes.
    let px = decoded.at_2d::<core::Vec3b>(0, 0).expect("pixel");
    assert_eq!([px[0], px[1], px[2]], [10, 20, 30]);
}
