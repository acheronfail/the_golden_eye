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
        rom_language: "en".to_owned(),
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

    let files = crate::db::clips::video_files_in_directory_recursive(&dir.path).unwrap();

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
    let runs = list_configured_runs(&settings, &catalog);

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

    let runs = list_configured_runs(&settings, &catalog);

    assert_eq!(runs.clips.len(), 2);
    assert_eq!(runs.clips[0].metadata.status, RunStatus::Complete);
    assert_eq!(runs.clips[1].metadata.status, RunStatus::Failed);
    assert!(runs.clips.iter().all(|clip| clip.duration_secs.is_some()));
}

#[test]
fn stream_configured_runs_refreshes_catalog_before_emitting() {
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

    let mut clips = Vec::new();
    stream_configured_runs(&settings, &catalog, true, |event| {
        if let RunsStreamEvent::Clip { clip } = event {
            clips.push(*clip);
        }
        true
    });

    assert_eq!(clips.len(), 2);
    assert_eq!(clips[0].file_name, "new.mov");
    assert_eq!(clips[0].metadata.timestamp, "2026-01-02T00:00:00Z");
    assert!(clips[1].path.is_empty(), "missing videos retain their history row");
}

#[test]
fn runs_stream_params_defaults_refresh_to_false() {
    let params: RunsStreamParams = serde_json::from_value(serde_json::json!({})).expect("missing refresh defaults");
    assert!(!params.refresh);
    let params: RunsStreamParams =
        serde_json::from_value(serde_json::json!({ "refresh": true })).expect("refresh parses");
    assert!(params.refresh);
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
                rom_language: "jp".to_owned(),
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
    assert_eq!(persisted.metadata.rom_language, "jp");
    assert_eq!(persisted.metadata.difficulty.as_deref(), Some("Agent"));
}
