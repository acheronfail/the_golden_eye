use ge_clip::{RomVersion, RunStatus};

use super::*;

fn sample_clip() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../frame_tests/clips/sample_clip.mov")
}

#[test]
fn reads_legacy_clip_language_tag() {
    let mut tags = Dictionary::new();
    tags.set(TAG_CREATED_BY, TAG_CREATED_BY_VALUE);
    tags.set(TAG_RUN_TIMESTAMP, "2026-07-24T12:00:00Z");
    tags.set(TAG_STATUS, "complete");
    tags.set(TAG_LEVEL, "Frigate");
    tags.set(TAG_LEGACY_ROM_LANGUAGE, "jp");

    let metadata = clip_metadata_from_ffmpeg_tags(&tags).unwrap();
    assert_eq!(metadata.game_language, "jp");
    assert_eq!(metadata.rom_version, None);
    assert!(!metadata.was_personal_best);
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

#[derive(Debug)]
struct PacketTiming {
    stream: usize,
    data: Vec<u8>,
    pts: f64,
    dts: f64,
    duration: f64,
    key: bool,
}

fn packet_timings(path: &Path) -> Vec<PacketTiming> {
    init().unwrap();
    let mut input = format::input(path).unwrap();
    input
        .packets()
        .map(|(stream, packet)| {
            let seconds = |value: i64| value as f64 * f64::from(stream.time_base());
            PacketTiming {
                stream: stream.index(),
                data: packet.data().unwrap().to_vec(),
                pts: seconds(packet.pts().unwrap()),
                dts: seconds(packet.dts().unwrap()),
                duration: seconds(packet.duration()),
                key: packet.is_key(),
            }
        })
        .collect()
}

#[test]
fn remux_preserves_packet_timing_and_audio_video_sync() {
    let input = sample_clip();
    let original = packet_timings(&input);
    assert!(original.iter().any(|packet| packet.pts != packet.dts), "fixture must have reordered frames");

    for (case, window) in [None, Some((0.0, 5.0)), Some((1.0, 5.0)), Some((3.25, 6.0))].into_iter().enumerate() {
        let out = std::env::temp_dir().join(format!("ge_packet_timing_{}_{case}.mp4", std::process::id()));
        remux_with_metadata(&input, &out, window, None).unwrap();
        let copied = packet_timings(&out);
        std::fs::remove_file(&out).unwrap();

        let mut common_shift: Option<f64> = None;
        for stream in 0..=1 {
            let source: Vec<_> = original.iter().filter(|packet| packet.stream == stream).collect();
            let result: Vec<_> = copied.iter().filter(|packet| packet.stream == stream).collect();
            assert!(result.len() > 2, "both audio and video must survive");
            let first = source.iter().position(|packet| packet.data == result[0].data).unwrap();
            if stream == 0 {
                assert!(result[0].key, "video must begin with a keyframe");
            }
            assert!(result.windows(2).all(|pair| pair[1].dts > pair[0].dts), "DTS must advance");
            for (index, packet) in result.iter().enumerate() {
                let before = source[first + index];
                assert_eq!(packet.data, before.data, "encoded packets must be unchanged");
                let shift = *common_shift.get_or_insert(packet.dts - before.dts);
                assert!((packet.dts - before.dts - shift).abs() < 0.002, "DTS spacing and A/V sync must survive");
                assert!((packet.pts - before.pts - shift).abs() < 0.002, "presentation order must survive");
                assert!((packet.duration - before.duration).abs() < 0.002, "packet duration must survive");
            }
        }
        if window.is_some() {
            let first_dts = copied.iter().map(|packet| packet.dts).fold(f64::INFINITY, f64::min);
            assert!(first_dts.abs() < 0.002, "trim must start near zero: {first_dts}");
        }
    }
}

#[test]
fn trims_with_metadata_and_reads_it_back() {
    let input = sample_clip();
    let full = duration_secs(&input).expect("probe duration");
    let out = std::env::temp_dir().join(format!("ge_ffmpeg_metadata_test_{}.mov", std::process::id()));
    let _ = std::fs::remove_file(&out);

    let metadata = ClipMetadata {
        run_id: "run-1".to_owned(),
        timestamp: "2026-01-02T03:04:05Z".to_owned(),
        time: Some("02:03".to_owned()),
        time_seconds: Some(123),
        level: "Surface 2".to_owned(),
        level_number: Some(8),
        difficulty: Some("00 Agent".to_owned()),
        status: RunStatus::Complete,
        was_personal_best: true,
        game_language: "en".to_owned(),
        rom_version: Some(RomVersion::Pal),
        source_name: "N64 Capture".to_owned(),
        comment: "Created by The Golden Eye OBS plugin v0.0.0".to_owned(),
        plugin_version: "0.0.0".to_owned(),
        retention_state: "pending".to_owned(),
        retention_reason: None,
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
        run_id: "run-1".to_owned(),
        timestamp: "2026-01-02T03:04:05Z".to_owned(),
        time: Some("02:03".to_owned()),
        time_seconds: Some(123),
        level: "Surface 2".to_owned(),
        level_number: Some(8),
        difficulty: Some("00 Agent".to_owned()),
        status: RunStatus::Complete,
        was_personal_best: false,
        game_language: "en".to_owned(),
        rom_version: Some(RomVersion::NtscU),
        source_name: "N64 Capture".to_owned(),
        comment: "Created by The Golden Eye OBS plugin v0.0.0".to_owned(),
        plugin_version: "0.0.0".to_owned(),
        retention_state: "pending".to_owned(),
        retention_reason: None,
    };
    trim_with_metadata(&input, &out, 1.0, (full - 1.0).max(2.0), Some(&original)).expect("trim with metadata");

    let updated = ClipMetadata {
        run_id: "run-1".to_owned(),
        timestamp: "2026-01-03T03:04:05Z".to_owned(),
        time: None,
        time_seconds: None,
        level: "Dam".to_owned(),
        level_number: Some(1),
        difficulty: None,
        status: RunStatus::Failed,
        was_personal_best: false,
        game_language: "jp".to_owned(),
        rom_version: Some(RomVersion::NtscJ),
        source_name: "N64 Capture".to_owned(),
        comment: "Created by The Golden Eye OBS plugin v0.0.0".to_owned(),
        plugin_version: "0.0.0".to_owned(),
        retention_state: "kept".to_owned(),
        retention_reason: Some("manual".to_owned()),
    };
    rewrite_metadata_in_place(&out, &updated).expect("rewrite metadata");

    let got = read_clip_metadata(&out).expect("read metadata").expect("plugin metadata");
    assert_eq!(got, updated);

    let _ = std::fs::remove_file(&out);
}
