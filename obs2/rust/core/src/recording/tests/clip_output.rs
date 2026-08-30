#[test]
fn output_dir_prefers_configured_path_then_replay_parent() {
    let dir = TestDir::new("output-dir");
    let input = dir.join("replay.mov");
    let completed = dir.join("completed");
    let mut options = RecordingOptions {
        completed_output_path: completed.to_string_lossy().into_owned(),
        ..RecordingOptions::default()
    };

    assert_eq!(output_dir(&input, &options), completed);

    options.completed_output_path.clear();
    assert_eq!(output_dir(&input, &options), dir.path());
}

#[test]
fn ensure_output_directory_creates_nested_missing_directory() {
    let dir = TestDir::new("ensure-output");
    let output = dir.join("completed/deeply/nested");

    assert!(!output.exists());
    ensure_output_directory(&output).unwrap();

    assert!(output.is_dir());
}

#[test]
fn ensure_output_directory_rejects_existing_file() {
    let dir = TestDir::new("ensure-output-file");
    let output = dir.join("completed");
    write_file(&output);

    let err = ensure_output_directory(&output).unwrap_err();

    assert!(
        err.to_string().contains("creating output directory")
            || err.to_string().contains("exists but is not a directory"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn unique_output_path_chooses_first_available_numeric_suffix() {
    let dir = TestDir::new("unique-output");
    let base = dir.join("clip.mp4");
    let second = dir.join("clip (2).mp4");
    write_file(&base);
    write_file(&second);

    let third = dir.join("clip (3).mp4");
    assert_eq!(unique_output_path(&base), third);
    assert!(!third.exists());

    let no_ext = dir.join("clip");
    write_file(&no_ext);
    assert_eq!(unique_output_path(&no_ext), dir.join("clip (2)"));
}

#[test]
fn render_clip_template_replaces_all_supported_tokens() {
    let m = match_with_time();
    let completed_at = UNIX_EPOCH + Duration::from_secs(1_700_000_000);

    let rendered = render_clip_template(
        "{obs_replay_name}|{mission}|{part}|{levelNumber}|{level}|{time}|{difficulty}|{status}|{timestamp}|{timestamp_local}",
        "obs replay",
        RunStatus::Complete,
        completed_at,
        Some(&m),
    );

    assert_eq!(
        rendered,
        format!(
            "obs replay|05|1|8|Surface 2|02:03|00 Agent|complete|2023-11-14T22:13:20Z|{}",
            format_iso_local(completed_at),
        ),
    );
}

#[test]
fn render_clip_template_uses_empty_fields_without_stats() {
    let rendered = render_clip_template(
        "{level}|{mission}|{part}|{levelNumber}|{time}|{difficulty}|{status}|{obs_replay_name}",
        "replay",
        RunStatus::Failed,
        UNIX_EPOCH,
        None,
    );

    assert_eq!(rendered, "unknown||||||failed|replay");
}

#[test]
fn render_clip_template_omits_time_when_report_has_no_stats_row() {
    let m = match_without_time();

    let rendered = render_clip_template(
        "{mission}-{part}-{levelNumber}-{level}-{time}-{difficulty}-{status}",
        "replay",
        RunStatus::Abort,
        UNIX_EPOCH,
        Some(&m),
    );

    assert_eq!(rendered, "01-2-2-Facility--Secret Agent-abort");
}

#[test]
fn render_clip_template_marks_unreadable_header_parts() {
    let m = match_with_unreadable_header();

    let rendered = render_clip_template(
        "{mission}|{part}|{levelNumber}|{level}|{time}|{difficulty}|{status}",
        "replay",
        RunStatus::Kia,
        UNIX_EPOCH,
        Some(&m),
    );

    assert_eq!(rendered, "??|?||unknown|00:00||kia");
}

#[test]
fn render_clip_template_leaves_unknown_tokens_and_unsanitized_text() {
    let m = match_with_time();

    let rendered = render_clip_template(
        "{obs_replay_name}/{not_a_token}/{level}:{status}",
        "OBS/Replay:01",
        RunStatus::Complete,
        UNIX_EPOCH,
        Some(&m),
    );

    assert_eq!(rendered, "OBS/Replay:01/{not_a_token}/Surface 2:complete");
}

#[test]
fn clip_template_renders_and_sanitizes_relative_paths() {
    let m = match_with_time();

    let rendered = render_clip_template(
        "{obs_replay_name}-{mission}-{part}-{levelNumber}-{level}-{time}-{difficulty}-{status}-{timestamp}",
        "obs replay",
        RunStatus::Abort,
        UNIX_EPOCH,
        Some(&m),
    );
    assert_eq!(rendered, "obs replay-05-1-8-Surface 2-02:03-00 Agent-abort-1970-01-01T00:00:00Z");

    let path = clip_relative_path(
        "OBS/Replay:01",
        RunStatus::Kia,
        UNIX_EPOCH,
        Some(&m),
        &format!(
            "{{level}}{}{{difficulty}}{}{{time}}?{{status}}",
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR
        ),
    );
    let name = path.file_name().and_then(|name| name.to_str()).unwrap();
    for forbidden in ['/', '\\', ':', '*', '?', '"', '<', '>', '|'] {
        assert!(!name.contains(forbidden), "{name:?} still contains {forbidden:?}");
    }
    assert_eq!(path.parent().unwrap(), Path::new("Surface 2").join("00 Agent"));
    assert!(name.contains("02-03"));
    assert!(name.ends_with("-kia"));

    assert_eq!(
        clip_relative_path("replay", RunStatus::Complete, UNIX_EPOCH, None, "..."),
        PathBuf::from(format!("unknown -  -  - {}", sanitize_path_component(&format_iso_local(UNIX_EPOCH)))),
    );
}

#[test]
fn clip_template_rejects_traversal_and_wrong_platform_separator() {
    let m = match_with_time();

    assert_eq!(
        clip_relative_path("replay", RunStatus::Complete, UNIX_EPOCH, Some(&m), "../{level}"),
        default_flat_clip_path_for_surface_2(UNIX_EPOCH),
    );
    assert_eq!(
        clip_relative_path(
            "replay",
            RunStatus::Complete,
            UNIX_EPOCH,
            Some(&m),
            &format!("{{level}}{}..{}{{time}}", std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR),
        ),
        default_flat_clip_path_for_surface_2(UNIX_EPOCH),
    );
    assert_eq!(
        clip_relative_path(
            "replay",
            RunStatus::Complete,
            UNIX_EPOCH,
            Some(&m),
            if std::path::MAIN_SEPARATOR == '/' { "{level}\\{time}" } else { "{level}/{time}" },
        ),
        default_flat_clip_path_for_surface_2(UNIX_EPOCH),
    );
}
