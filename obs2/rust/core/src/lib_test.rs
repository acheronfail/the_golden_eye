/// Smoke test that the statically-linked FFmpeg is actually callable from
/// Rust (i.e. the libav* symbols resolve at link time). `version()` just
/// reads a compiled-in constant, so this purely exercises the linkage.
#[test]
fn ffmpeg_links_and_initializes() {
    ffmpeg_next::init().expect("ffmpeg init");
    let v = ffmpeg_next::format::version();
    assert!(v > 0, "libavformat version should be non-zero");
}

#[test]
fn consumed_staged_update_is_idle_after_reload() {
    let status = super::initial_update_status(true, true);

    assert_eq!(status.phase, crate::updates::UpdatePhase::Idle);
}

#[test]
fn staged_update_is_preserved_on_cold_start() {
    let status = super::initial_update_status(false, true);

    assert_eq!(status.phase, crate::updates::UpdatePhase::Staged);
}

#[test]
fn auto_start_monitor_requires_an_enabled_available_last_source() {
    use crate::http::routes::sources::Source;
    use crate::settings::AppSettings;

    let sources = vec![Source { name: "N64 Capture".to_owned(), id: "av_capture_input".to_owned() }];
    let mut settings = AppSettings::default();

    assert_eq!(super::auto_start_monitor_source(&settings, &sources), None);

    settings.auto_start_monitor_on_launch = true;
    assert_eq!(super::auto_start_monitor_source(&settings, &sources), None);

    settings.last_used_source_name = Some("Disconnected Capture".to_owned());
    assert_eq!(super::auto_start_monitor_source(&settings, &sources), None);

    settings.last_used_source_name = Some("N64 Capture".to_owned());
    assert_eq!(super::auto_start_monitor_source(&settings, &sources).as_deref(), Some("N64 Capture"));
}
