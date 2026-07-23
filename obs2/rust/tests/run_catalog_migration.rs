mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use support::harness::{API, Harness, recording_settings, run_catalog_path};

fn sample_clip(root: &Path) -> PathBuf {
    root.join("test/clips/sample_clip.mov")
}

fn write_tagged_clip(root: &Path, path: &Path, status: &str, timestamp: &str) {
    ge_rust::ge_test_write_tagged_clip(&sample_clip(root), path, status, timestamp);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn schema_one_catalog_is_reset_and_existing_clips_are_reseeded_as_kept() {
    let harness = Harness::start_with_settings_from_temp(Duration::ZERO, |temp| {
        let completed = temp.join("clips");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        write_tagged_clip(&root, &completed.join("old.mov"), "failed", "2026-01-01T00:00:00Z");
        write_tagged_clip(&root, &completed.join("new.mov"), "complete", "2026-01-02T00:00:00Z");
        let database = run_catalog_path(temp);
        std::fs::create_dir_all(database.parent().unwrap()).unwrap();
        let conn = rusqlite::Connection::open(database).unwrap();
        conn.execute_batch("CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT INTO meta VALUES ('schema_version', '1'); CREATE TABLE stale (value TEXT);").unwrap();
        recording_settings(&completed, &temp.join("unused"))
    })
    .await;

    let completed = harness.temp.join("clips");
    assert!(completed.join("old.mov").exists(), "clip seeding is lazy until Runs is opened");

    let runs: Value = harness.client.get(format!("{API}/api/v1/runs")).send().await.unwrap().json().await.unwrap();
    assert!(completed.join("old.mov").exists());
    assert!(completed.join("new.mov").exists());
    let clips = runs["clips"].as_array().unwrap();
    assert_eq!(clips.len(), 2);
    assert_eq!(clips[0]["metadata"]["status"], "complete");
    assert_eq!(clips[0]["retentionState"], "kept");
    assert_eq!(clips[1]["metadata"]["status"], "failed");
    assert_eq!(clips[1]["retentionState"], "kept");
}
