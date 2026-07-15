use super::*;

fn sample_clip() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/clips/sample_clip.mov")
}

#[test]
fn reads_duration() {
    let dur = duration_secs(&sample_clip()).expect("probe duration");
    assert!(dur > 1.0, "sample clip should be longer than a second, got {dur}");
}

#[test]
fn trims_to_requested_window() {
    let input = sample_clip();
    let full = duration_secs(&input).expect("probe duration");

    // Trim a window comfortably inside the clip and confirm the output is a
    // valid container of roughly the requested length. Keyframe-aligned cuts
    // mean the real bounds drift a little, so the tolerance is generous.
    let (start, end) = (1.0, (full - 1.0).max(2.0));
    let want = end - start;

    let out = std::env::temp_dir().join("ge_ffmpeg_trim_test.mov");
    let _ = std::fs::remove_file(&out);
    trim(&input, &out, start, end).expect("trim");

    let got = duration_secs(&out).expect("probe trimmed duration");
    assert!((got - want).abs() < 1.5, "trimmed duration {got:.3}s should be near requested {want:.3}s",);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn trims_with_metadata_and_reads_it_back() {
    let input = sample_clip();
    let full = duration_secs(&input).expect("probe duration");
    let out = std::env::temp_dir().join(format!("ge_ffmpeg_metadata_test_{}.mov", std::process::id()));
    let _ = std::fs::remove_file(&out);

    let metadata = ClipMetadata {
        timestamp: "2026-01-02T03:04:05Z".to_owned(),
        time: Some("02:03".to_owned()),
        time_seconds: Some(123),
        level: "Surface 2".to_owned(),
        level_number: Some(8),
        difficulty: Some("00 Agent".to_owned()),
        status: "complete".to_owned(),
        rom_language: "en".to_owned(),
        source_name: "N64 Capture".to_owned(),
        comment: "Created by The Golden Eye OBS plugin v0.0.0".to_owned(),
        plugin_version: "0.0.0".to_owned(),
    };

    trim_with_metadata(&input, &out, 1.0, (full - 1.0).max(2.0), Some(&metadata)).expect("trim with metadata");

    let got = read_clip_metadata(&out).expect("read metadata").expect("plugin metadata");
    assert_eq!(got, metadata);

    let _ = std::fs::remove_file(&out);
}

#[test]
fn rewrites_metadata_in_place_and_drops_old_optional_tags() {
    let input = sample_clip();
    let full = duration_secs(&input).expect("probe duration");
    let out = std::env::temp_dir().join(format!("ge_ffmpeg_metadata_rewrite_test_{}.mov", std::process::id()));
    let _ = std::fs::remove_file(&out);

    let original = ClipMetadata {
        timestamp: "2026-01-02T03:04:05Z".to_owned(),
        time: Some("02:03".to_owned()),
        time_seconds: Some(123),
        level: "Surface 2".to_owned(),
        level_number: Some(8),
        difficulty: Some("00 Agent".to_owned()),
        status: "complete".to_owned(),
        rom_language: "en".to_owned(),
        source_name: "N64 Capture".to_owned(),
        comment: "Created by The Golden Eye OBS plugin v0.0.0".to_owned(),
        plugin_version: "0.0.0".to_owned(),
    };
    trim_with_metadata(&input, &out, 1.0, (full - 1.0).max(2.0), Some(&original)).expect("trim with metadata");

    let updated = ClipMetadata {
        timestamp: "2026-01-03T03:04:05Z".to_owned(),
        time: None,
        time_seconds: None,
        level: "Dam".to_owned(),
        level_number: Some(1),
        difficulty: None,
        status: "failed".to_owned(),
        rom_language: "jp".to_owned(),
        source_name: "N64 Capture".to_owned(),
        comment: "Created by The Golden Eye OBS plugin v0.0.0".to_owned(),
        plugin_version: "0.0.0".to_owned(),
    };
    rewrite_metadata_in_place(&out, &updated).expect("rewrite metadata");

    let got = read_clip_metadata(&out).expect("read metadata").expect("plugin metadata");
    assert_eq!(got, updated);

    let _ = std::fs::remove_file(&out);
}
