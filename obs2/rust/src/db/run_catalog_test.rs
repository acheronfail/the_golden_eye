use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::db::run_catalog::RunCatalog;
use crate::ffmpeg;
use crate::models::clip_metadata::RunStatus;

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(label: &str) -> Self {
        loop {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!("ge-run-catalog-{label}-{}-{nanos}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return TestDir { path },
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
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

fn sample_clip() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/clips/sample_clip.mov")
}

fn metadata(status: &str, timestamp: &str) -> ClipMetadata {
    ClipMetadata {
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
    }
}

fn write_tagged_clip(path: &Path, status: &str, timestamp: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let input = sample_clip();
    let full = ffmpeg::duration_secs(&input).expect("probe sample clip");
    let metadata = metadata(status, timestamp);
    ffmpeg::trim_with_metadata(&input, path, 1.0, (full - 1.0).max(2.0), Some(&metadata))
        .expect("write tagged test clip");
}

fn catalog(dir: &TestDir) -> RunCatalog {
    RunCatalog::open(dir.join("runs.sqlite")).expect("open catalog")
}

#[test]
fn seed_from_roots_indexes_nested_tagged_clips() {
    let dir = TestDir::new("seed-nested");
    let completed = dir.join("completed");
    let failed = dir.join("failed");
    let complete_clip = completed.join("Dam/Agent/complete.mov");
    let failed_clip = failed.join("Facility/00 Agent/failed.mov");
    write_tagged_clip(&complete_clip, "complete", "2026-01-02T00:00:00Z");
    write_tagged_clip(&failed_clip, "failed", "2026-01-01T00:00:00Z");
    fs::write(failed.join("Facility/notes.txt"), b"ignored").unwrap();

    let catalog = catalog(&dir);
    catalog.resync(&[RunCatalogRoot { path: completed.clone() }, RunCatalogRoot { path: failed.clone() }]).unwrap();

    let clips = catalog.list(&[RunCatalogRoot { path: completed }, RunCatalogRoot { path: failed }]).unwrap();
    assert_eq!(clips.len(), 2);
    assert_eq!(clips[0].metadata.status, RunStatus::Complete);
    assert_eq!(clips[1].metadata.status, RunStatus::Failed);
    assert!(clips.iter().all(|clip| clip.duration_secs.is_some()));
}

#[test]
fn failed_clip_reviews_persist_and_keep_never_deletes_the_clip() {
    let dir = TestDir::new("failed-reviews");
    let failed = dir.join("failed");
    let clip = failed.join("Dam/Agent/failed.mov");
    write_tagged_clip(&clip, "failed", "2026-01-01T00:00:00Z");

    let run_catalog = catalog(&dir);
    run_catalog.resync(&[RunCatalogRoot { path: failed }]).unwrap();
    let pending = run_catalog.pending_failed_reviews().unwrap();
    assert_eq!(pending.len(), 1);

    run_catalog.keep_failed_reviews(&[pending[0].path.clone()]).unwrap();
    assert!(run_catalog.pending_failed_reviews().unwrap().is_empty());
    assert!(clip.exists(), "keeping a failed clip must never remove the file");

    drop(run_catalog);
    let reopened = catalog(&dir);
    assert!(reopened.pending_failed_reviews().unwrap().is_empty());
    assert!(clip.exists());
}

#[test]
fn record_rename_update_and_remove_keep_catalog_in_sync() {
    let dir = TestDir::new("mutations");
    let root = dir.join("completed");
    let clip_path = root.join("clip.mov");
    write_tagged_clip(&clip_path, "complete", "2026-01-01T00:00:00Z");
    let catalog = catalog(&dir);
    let clip_metadata = ffmpeg::read_clip_metadata(&clip_path).unwrap().unwrap();
    catalog
        .record_saved_clip(RunCatalogSave {
            path: clip_path.clone(),
            duration_secs: Some(1.5),
            metadata: clip_metadata.clone(),
        })
        .unwrap();

    let renamed = root.join("renamed.mov");
    fs::rename(&clip_path, &renamed).unwrap();
    catalog.rename_path(&clip_path, &renamed).unwrap();
    let mut updated = clip_metadata;
    updated.status = RunStatus::Failed;
    ffmpeg::rewrite_metadata_in_place(&renamed, &updated).unwrap();
    catalog.refresh_clip(&renamed).unwrap();

    let clips = catalog.list(&[RunCatalogRoot { path: root.clone() }]).unwrap();
    assert_eq!(clips.len(), 1);
    assert_eq!(clips[0].path, fs::canonicalize(&renamed).unwrap());
    assert_eq!(clips[0].metadata.status, RunStatus::Failed);

    fs::remove_file(&renamed).unwrap();
    catalog.remove_path(&renamed).unwrap();
    assert!(catalog.list(&[RunCatalogRoot { path: root }]).unwrap().is_empty());
}

#[test]
fn resync_removes_missing_and_out_of_root_entries() {
    let dir = TestDir::new("resync-removes");
    let root = dir.join("completed");
    let outside = dir.join("outside");
    let deleted = root.join("deleted.mov");
    let moved = root.join("moved.mov");
    write_tagged_clip(&deleted, "complete", "2026-01-01T00:00:00Z");
    write_tagged_clip(&moved, "complete", "2026-01-02T00:00:00Z");
    let catalog = catalog(&dir);
    let roots = [RunCatalogRoot { path: root.clone() }];
    catalog.resync(&roots).unwrap();
    assert_eq!(catalog.list(&roots).unwrap().len(), 2);

    fs::remove_file(&deleted).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::rename(&moved, outside.join("moved.mov")).unwrap();
    catalog.resync(&roots).unwrap();

    assert!(catalog.list(&roots).unwrap().is_empty());
}

#[test]
fn resync_discovers_new_external_clips_and_ignores_untagged_videos() {
    let dir = TestDir::new("resync-new");
    let root = dir.join("completed");
    let clip = root.join("Surface 2/Agent/new.mov");
    let untagged = root.join("untagged.mov");
    let catalog = catalog(&dir);
    let roots = [RunCatalogRoot { path: root.clone() }];
    catalog.resync(&roots).unwrap();
    assert!(catalog.list(&roots).unwrap().is_empty());

    write_tagged_clip(&clip, "complete", "2026-01-01T00:00:00Z");
    fs::copy(sample_clip(), &untagged).unwrap();
    catalog.resync(&roots).unwrap();

    let clips = catalog.list(&roots).unwrap();
    assert_eq!(clips.len(), 1);
    assert_eq!(clips[0].path, fs::canonicalize(&clip).unwrap());
}

#[test]
fn resync_refreshes_externally_rewritten_metadata() {
    let dir = TestDir::new("resync-metadata");
    let root = dir.join("completed");
    let clip = root.join("clip.mov");
    write_tagged_clip(&clip, "complete", "2026-01-01T00:00:00Z");
    let catalog = catalog(&dir);
    let roots = [RunCatalogRoot { path: root.clone() }];
    catalog.resync(&roots).unwrap();
    assert_eq!(catalog.list(&roots).unwrap()[0].metadata.status, RunStatus::Complete);

    write_tagged_clip(&clip, "failed", "2026-01-02T00:00:00Z");
    catalog.resync(&roots).unwrap();

    let clips = catalog.list(&roots).unwrap();
    assert_eq!(clips.len(), 1);
    assert_eq!(clips[0].metadata.status, RunStatus::Failed);
    assert_eq!(clips[0].metadata.timestamp, "2026-01-02T00:00:00Z");
}

#[test]
fn resync_removes_externally_corrupted_clips() {
    let dir = TestDir::new("resync-corrupt");
    let root = dir.join("completed");
    let clip = root.join("clip.mov");
    write_tagged_clip(&clip, "complete", "2026-01-01T00:00:00Z");
    let catalog = catalog(&dir);
    let roots = [RunCatalogRoot { path: root.clone() }];
    catalog.resync(&roots).unwrap();
    assert_eq!(catalog.list(&roots).unwrap().len(), 1);

    fs::write(&clip, b"not a video anymore").unwrap();
    catalog.resync(&roots).unwrap();

    assert!(catalog.list(&roots).unwrap().is_empty());
}

fn youtube_metadata(video_id: &str) -> crate::youtube::YoutubeMetadata {
    crate::youtube::YoutubeMetadata {
        video_id: video_id.to_owned(),
        video_url: format!("https://youtu.be/{video_id}"),
        uploaded_at: format!("2026-01-0{}T00:00:00Z", if video_id.ends_with('1') { 1 } else { 2 }),
        title: format!("Video {video_id}"),
    }
}

#[test]
fn youtube_history_round_trips_and_forgets_by_path() {
    let dir = TestDir::new("youtube-history");
    let catalog = catalog(&dir);
    let clip = dir.join("clip.mov");
    write_tagged_clip(&clip, "complete", "2026-01-01T00:00:00Z");
    let roots = [RunCatalogRoot { path: dir.path.clone() }];
    catalog.resync(&roots).unwrap();
    let entry = youtube_metadata("video-1");
    let mut updated = entry.clone();
    updated.title = "Updated title".to_owned();

    catalog.set_youtube_history(&clip, &entry).unwrap();
    catalog.set_youtube_history(&clip, &updated).unwrap();
    catalog.resync(&roots).unwrap();
    let history = catalog.youtube_history().unwrap();
    let catalog_path = fs::canonicalize(&clip).unwrap().to_string_lossy().into_owned();
    assert_eq!(history, vec![UploadHistoryEntry { path: catalog_path, youtube: updated }]);

    let conn = rusqlite::Connection::open(dir.join("runs.sqlite")).unwrap();
    let stored: Option<String> = conn.query_row("SELECT youtube_json FROM clips", [], |row| row.get(0)).unwrap();
    assert!(stored.is_some());

    assert_eq!(catalog.forget_youtube_history_for_display_path(&clip.to_string_lossy()).unwrap(), 1);
    assert!(catalog.youtube_history().unwrap().is_empty());
}

#[test]
fn sqlite_metadata_document_round_trips_complete_metadata() {
    let dir = TestDir::new("sqlite-document");
    let root = dir.join("completed");
    let clip = root.join("full.mov");
    write_tagged_clip(&clip, "complete", "2026-01-01T00:00:00Z");
    let catalog = catalog(&dir);
    let roots = [RunCatalogRoot { path: root.clone() }];
    catalog.resync(&roots).unwrap();

    let clips = catalog.list(&roots).unwrap();
    assert_eq!(clips.len(), 1);
    assert_eq!(
        clips[0].metadata,
        ClipMetadata {
            timestamp: "2026-01-01T00:00:00Z".to_owned(),
            time: Some("02:03".to_owned()),
            time_seconds: Some(123),
            level: "Surface 2".to_owned(),
            level_number: Some(8),
            difficulty: Some("00 Agent".to_owned()),
            status: RunStatus::Complete,
            rom_language: "en".to_owned(),
            source_name: "N64 Capture".to_owned(),
            comment: "Created by The Golden Eye OBS plugin test".to_owned(),
            plugin_version: "test".to_owned(),
        }
    );

    drop(catalog);
    let conn = rusqlite::Connection::open(dir.join("runs.sqlite")).unwrap();
    let stored_json: String = conn.query_row("SELECT metadata_json FROM clips", [], |row| row.get(0)).unwrap();
    assert_eq!(serde_json::from_str::<ClipMetadata>(&stored_json).unwrap(), clips[0].metadata);

    let mut statement = conn.prepare("PRAGMA table_info(clips)").unwrap();
    let columns =
        statement.query_map([], |row| row.get::<_, String>(1)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(columns, ["path", "size_bytes", "modified_unix", "duration_secs", "metadata_json", "youtube_json"]);

    let mut statement = conn.prepare("PRAGMA index_list(clips)").unwrap();
    let indexes =
        statement.query_map([], |row| row.get::<_, String>(1)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    for index in ["clips_status_timestamp_idx", "clips_level_difficulty_timestamp_idx", "clips_time_idx"] {
        assert!(indexes.iter().any(|candidate| candidate == index), "missing expression index {index}");
    }
}

#[test]
fn unsupported_schema_version_drops_and_reseeds_without_failing_open() {
    let dir = TestDir::new("schema-reseed");
    let db_path = dir.join("runs.sqlite");
    let root = dir.join("completed");
    let clip = root.join("full.mov");
    write_tagged_clip(&clip, "complete", "2026-01-01T00:00:00Z");

    // Seed a catalog, then stamp it with a future schema version the binary can't use.
    {
        let catalog = RunCatalog::open(db_path.clone()).unwrap();
        catalog.resync(&[RunCatalogRoot { path: root.clone() }]).unwrap();
        assert_eq!(catalog.list(&[RunCatalogRoot { path: root.clone() }]).unwrap().len(), 1);
    }
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("UPDATE meta SET value = ?1 WHERE key = 'schema_version'", ["9999"]).unwrap();
    }

    // Reopening must succeed (never fail plugin startup) and start from a fresh, empty catalog.
    let catalog = RunCatalog::open(db_path).expect("reopen must not fail on schema mismatch");
    assert!(catalog.list(&[RunCatalogRoot { path: root.clone() }]).unwrap().is_empty());
    // The dropped catalog reseeds from disk on the next resync.
    catalog.resync(&[RunCatalogRoot { path: root.clone() }]).unwrap();
    assert_eq!(catalog.list(&[RunCatalogRoot { path: root }]).unwrap().len(), 1);
}
