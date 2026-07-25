use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use super::*;

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(label: &str) -> Self {
        loop {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!("ge-runs-{label}-{}-{nanos}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return TestDir { path },
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create test dir {}: {err}", path.display()),
            }
        }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn test_catalog(dir: &TestDir) -> crate::db::run_catalog::RunCatalog {
    crate::db::run_catalog::RunCatalog::open(dir.join("runs.sqlite")).expect("open run catalog")
}

fn sample_clip() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/clips/sample_clip.mov")
}

fn test_clip_metadata(status: &str, timestamp: &str) -> ClipMetadata {
    ClipMetadata {
        run_id: String::new(),
        timestamp: timestamp.to_owned(),
        time: Some("02:03".to_owned()),
        time_seconds: Some(123),
        level: "Surface 2".to_owned(),
        level_number: Some(8),
        difficulty: Some("00 Agent".to_owned()),
        status: status.parse().expect("valid run status"),
        game_language: "en".to_owned(),
        rom_version: None,
        source_name: "N64 Capture".to_owned(),
        comment: "Created by The Golden Eye OBS plugin test".to_owned(),
        plugin_version: "test".to_owned(),
        retention_state: "kept".to_owned(),
        retention_reason: Some("imported".to_owned()),
    }
}

fn write_tagged_clip(path: &Path, status: &str, timestamp: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let input = sample_clip();
    let full = ffmpeg::duration_secs(&input).expect("probe sample clip");
    let metadata = test_clip_metadata(status, timestamp);
    ffmpeg::trim_with_metadata(&input, path, 1.0, (full - 1.0).max(2.0), Some(&metadata))
        .expect("write tagged test clip");
}

#[test]
fn normalize_time_formats_mm_ss_and_seconds() {
    assert_eq!(normalize_time("1:02").unwrap(), Some((62, "01:02".to_owned())));
    assert_eq!(normalize_time("12:34").unwrap(), Some((754, "12:34".to_owned())));
    assert_eq!(normalize_time(" ").unwrap(), None);
}

#[test]
fn normalize_time_rejects_bad_values() {
    assert!(matches!(normalize_time("1"), Err(RunPathError::BadRequest(_))));
    assert!(matches!(normalize_time("1:2"), Err(RunPathError::BadRequest(_))));
    assert!(matches!(normalize_time("1:60"), Err(RunPathError::BadRequest(_))));
}

#[test]
fn manual_history_run_keeps_origin_and_youtube_metadata_without_a_clip() {
    let dir = TestDir::new("manual-history");
    let catalog = test_catalog(&dir);
    let run = create_manual_run(
        &catalog,
        ManualRunRequest {
            date: "2025-04-03".to_owned(),
            level: "Facility".to_owned(),
            difficulty: "00 Agent".to_owned(),
            time: "1:23".to_owned(),
            game_language: "en".to_owned(),
            rom_version: Some(RomVersion::Pal),
            youtube_url: Some("https://youtu.be/abc_123".to_owned()),
        },
    )
    .unwrap();

    assert!(run.path.is_empty());
    assert_eq!(run.metadata.time.as_deref(), Some("01:23"));
    assert_eq!(run.metadata.time_seconds, Some(83));
    assert_eq!(run.metadata.rom_version, Some(RomVersion::Pal));
    assert_eq!(run.retention_reason.as_deref(), Some("manualEntry"));
    assert_eq!(run.youtube.as_ref().map(|video| video.video_id.as_str()), Some("abc_123"));
    let stored = catalog.get_run(&run.run_id).unwrap().unwrap();
    assert_eq!(stored.youtube, run.youtube);
}

#[test]
fn elite_history_import_is_idempotent_and_preserves_source_metadata() {
    let dir = TestDir::new("elite-history");
    let catalog = test_catalog(&dir);
    let elite = crate::the_elite::EliteRun {
        time_id: "309706".to_owned(),
        timestamp: "2026-07-24T12:00:00Z".to_owned(),
        level: "Frigate".to_owned(),
        difficulty: "Agent".to_owned(),
        time: "0:33".to_owned(),
        time_seconds: 33,
        system: "NTSC-J".to_owned(),
        current_personal_best: true,
        proof_available: true,
        video_id: Some("bgddOpQBKk4".to_owned()),
    };

    let first = import_elite_runs(&catalog, "acheronfail", vec![elite.clone()]).unwrap();
    let second = import_elite_runs(&catalog, "acheronfail", vec![elite]).unwrap();

    assert_eq!((first.imported, first.already_imported, first.videos), (1, 0, 1));
    assert_eq!((second.imported, second.already_imported, second.videos), (0, 1, 0));
    let run = catalog.get_run("the-elite-309706").unwrap().unwrap();
    assert_eq!(run.retention_reason.as_deref(), Some("theElite"));
    assert_eq!(run.metadata.source_name, "The Elite (NTSC-J)");
    assert_eq!(run.metadata.game_language, "jp");
    assert_eq!(run.metadata.rom_version, Some(RomVersion::NtscJ));
    assert!(run.metadata.comment.contains("current personal best"));
    assert_eq!(run.youtube.as_ref().map(|video| video.video_id.as_str()), Some("bgddOpQBKk4"));
}

#[test]
fn elite_rom_versions_map_known_systems_and_leave_unknown_unset() {
    assert_eq!(elite_rom_version("NTSC"), Some(RomVersion::NtscU));
    assert_eq!(elite_rom_version("NTSC-U"), Some(RomVersion::NtscU));
    assert_eq!(elite_rom_version("ntsc-j"), Some(RomVersion::NtscJ));
    assert_eq!(elite_rom_version("PAL"), Some(RomVersion::Pal));
    assert_eq!(elite_rom_version("Unknown"), None);
}

#[test]
fn missing_elite_users_map_to_not_found_without_masking_other_upstream_errors() {
    let missing = anyhow::Error::new(crate::the_elite::UserNotFound::new("missing-runner"));
    assert_eq!(elite_fetch_error_status(&missing), StatusCode::NOT_FOUND);
    assert_eq!(missing.to_string(), "The Elite user ~missing-runner was not found");

    let upstream = anyhow::anyhow!("The Elite returned 503 Service Unavailable");
    assert_eq!(elite_fetch_error_status(&upstream), StatusCode::BAD_GATEWAY);
}

#[test]
fn manual_youtube_links_accept_only_video_urls() {
    assert_eq!(
        youtube_metadata("https://www.youtube.com/watch?v=abc-123", "2026-01-01T00:00:00Z", "title").unwrap().video_id,
        "abc-123"
    );
    assert!(youtube_metadata("https://example.com/watch?v=abc-123", "2026-01-01T00:00:00Z", "title").is_err());
    assert!(youtube_metadata("https://youtube.com/channel/abc-123", "2026-01-01T00:00:00Z", "title").is_err());
}

#[test]
fn normalized_run_file_name_preserves_extension_when_missing() {
    let path = Path::new("/runs/original.mov");
    assert_eq!(normalized_run_file_name(path, "renamed").unwrap(), "renamed.mov");
    assert_eq!(normalized_run_file_name(path, "renamed.mp4").unwrap(), "renamed.mp4");
}

#[test]
fn normalized_run_file_name_rejects_paths_and_non_video_extensions() {
    let path = Path::new("/runs/original.mov");
    assert!(matches!(normalized_run_file_name(path, "../renamed.mov"), Err(RunPathError::BadRequest(_))));
    assert!(matches!(normalized_run_file_name(path, "renamed.txt"), Err(RunPathError::BadRequest(_))));
}

#[test]
fn supported_video_extensions_are_shared_with_the_catalog() {
    for extension in ["mp4", "mov", "m4v", "mkv", "webm", "flv", "ts", "avi", "mpg", "mpeg"] {
        assert!(is_video_file(Path::new(&format!("clip.{extension}"))));
    }
    assert!(!is_video_file(Path::new("clip.txt")));
}

#[test]
fn video_files_in_directory_searches_recursively() {
    let dir = TestDir::new("recursive-video-files");
    let nested = dir.join("Surface 2/00 Agent");
    fs::create_dir_all(&nested).unwrap();
    let root_clip = dir.join("root.mov");
    let nested_clip = nested.join("02-03.mp4");
    let ignored = nested.join("notes.txt");
    fs::write(&root_clip, b"root").unwrap();
    fs::write(&nested_clip, b"nested").unwrap();
    fs::write(&ignored, b"ignored").unwrap();

    let files = crate::db::runs::video_files_in_directory_recursive(&dir.path).unwrap();

    let mut expected = vec![root_clip, nested_clip];
    expected.sort();
    assert_eq!(files, expected);
}

#[test]
fn list_configured_runs_creates_missing_output_directories_before_scanning() {
    let dir = TestDir::new("configured-missing");
    let completed = dir.join("completed/deeply/nested");
    let settings =
        AppSettings { completed_output_path: completed.to_string_lossy().into_owned(), ..AppSettings::default() };

    let catalog = test_catalog(&dir);
    let runs = list_configured_runs(&settings, &catalog, RunSort::Newest);

    assert!(completed.is_dir());
    assert!(runs.clips.is_empty());
    assert_eq!(runs.directories.len(), 1);
    assert_eq!(runs.directories[0].kind, RunDirectoryKind::Completed);
    assert_eq!(runs.directories[0].path, completed.to_string_lossy());
    assert!(runs.directories[0].exists);
    assert_eq!(runs.directories[0].error, None);
}

#[test]
fn list_configured_runs_reads_seeded_catalog_without_rescanning() {
    let dir = TestDir::new("catalog-list");
    let completed = dir.join("completed");
    let completed_clip = completed.join("Surface 2/00 Agent/complete.mov");
    let failed_clip = completed.join("Dam/Agent/failed.mov");
    write_tagged_clip(&completed_clip, "complete", "2026-01-02T00:00:00Z");
    write_tagged_clip(&failed_clip, "failed", "2026-01-01T00:00:00Z");
    let settings =
        AppSettings { completed_output_path: completed.to_string_lossy().into_owned(), ..AppSettings::default() };
    let catalog = test_catalog(&dir);
    seed_catalog_from_settings(&catalog, &settings).unwrap();

    let runs = list_configured_runs(&settings, &catalog, RunSort::Newest);

    assert_eq!(runs.clips.len(), 2);
    assert_eq!(runs.clips[0].metadata.status, RunStatus::Complete);
    assert_eq!(runs.clips[1].metadata.status, RunStatus::Failed);
    assert!(runs.clips.iter().all(|clip| clip.duration_secs.is_some()));
}

#[test]
fn refresh_catalog_updates_listing_without_deleting_missing_history() {
    let dir = TestDir::new("stream-refresh");
    let completed = dir.join("completed");
    let old_clip = completed.join("old.mov");
    let new_clip = completed.join("new.mov");
    write_tagged_clip(&old_clip, "complete", "2026-01-01T00:00:00Z");
    let settings =
        AppSettings { completed_output_path: completed.to_string_lossy().into_owned(), ..AppSettings::default() };
    let catalog = test_catalog(&dir);
    seed_catalog_from_settings(&catalog, &settings).unwrap();
    fs::remove_file(&old_clip).unwrap();
    write_tagged_clip(&new_clip, "complete", "2026-01-02T00:00:00Z");

    refresh_catalog_from_settings(&catalog, &settings).unwrap();
    let clips = list_configured_runs(&settings, &catalog, RunSort::Newest).clips;

    assert_eq!(clips.len(), 2);
    assert_eq!(clips[0].file_name, "new.mov");
    assert_eq!(clips[0].metadata.timestamp, "2026-01-02T00:00:00Z");
    assert!(clips[1].path.is_empty(), "missing videos retain their history row");
}

#[test]
fn runs_params_default_and_parse_refresh_and_sort() {
    let params: RunsParams = serde_json::from_value(serde_json::json!({})).expect("missing refresh defaults");
    assert!(!params.refresh);
    assert_eq!(params.sort, RunSort::Newest);
    let params: RunsParams =
        serde_json::from_value(serde_json::json!({ "refresh": true, "sort": "fastest" })).expect("params parse");
    assert!(params.refresh);
    assert_eq!(params.sort, RunSort::Fastest);
}

#[test]
fn metadata_updates_persist_for_runs_without_video() {
    let dir = TestDir::new("metadata-only-update");
    let catalog = test_catalog(&dir);
    let run = catalog
        .create_finalized_run(
            UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            test_clip_metadata("complete", "2023-11-14T22:13:20Z"),
        )
        .expect("create finalized run");

    let updated = update_run_metadata(
        &catalog,
        RunMetadataUpdateRequest {
            run_id: run.run_id.clone(),
            metadata: EditableRunMetadata {
                game_language: "jp".to_owned(),
                rom_version: Some(RomVersion::NtscJ),
                status: "failed".to_owned(),
                difficulty: "Agent".to_owned(),
                time: "01:02".to_owned(),
                level: "Dam".to_owned(),
            },
        },
    )
    .expect("update metadata-only run");

    assert_eq!(updated.run_id, run.run_id);
    assert!(updated.path.is_empty());
    assert_eq!(updated.metadata.level, "Dam");
    assert_eq!(updated.metadata.level_number, Some(1));
    assert_eq!(updated.metadata.time_seconds, Some(62));
    assert_eq!(updated.metadata.status, RunStatus::Failed);

    let persisted = catalog.get_run(&updated.run_id).unwrap().expect("persisted run");
    assert!(persisted.clip.is_none());
    assert_eq!(persisted.metadata.game_language, "jp");
    assert_eq!(persisted.metadata.rom_version, Some(RomVersion::NtscJ));
    assert_eq!(persisted.metadata.difficulty.as_deref(), Some("Agent"));
}
