/// Smoke test that the statically-linked FFmpeg is actually callable from
/// Rust (i.e. the libav* symbols resolve at link time). `version()` just
/// reads a compiled-in constant, so this purely exercises the linkage.
#[test]
fn ffmpeg_links_and_initializes() {
    ffmpeg_next::init().expect("ffmpeg init");
    let v = ffmpeg_next::format::version();
    assert!(v > 0, "libavformat version should be non-zero");
}
