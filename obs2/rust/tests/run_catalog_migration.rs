mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use support::harness::{API, Harness, recording_settings};

fn sample_clip(root: &Path) -> PathBuf {
    root.join("test/clips/sample_clip.mov")
}

fn write_tagged_clip(root: &Path, path: &Path, status: &str, timestamp: &str) {
    ge_rust::ge_test_write_tagged_clip(&sample_clip(root), path, status, timestamp);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn startup_seeds_missing_run_catalog_and_prunes_nested_failed_clips() {
    let harness = Harness::start_with_settings_from_temp(Duration::ZERO, |temp| {
        let completed = temp.join("clips");
        let failed = temp.join("failed");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        write_tagged_clip(&root, &failed.join("Dam/Agent/old.mov"), "failed", "2026-01-01T00:00:00Z");
        write_tagged_clip(&root, &failed.join("Facility/Agent/middle.mov"), "kia", "2026-01-02T00:00:00Z");
        write_tagged_clip(&root, &failed.join("Runway/Agent/newest.mov"), "abort", "2026-01-03T00:00:00Z");
        write_tagged_clip(&root, &failed.join("Surface/Agent/complete.mov"), "complete", "2026-01-04T00:00:00Z");
        let mut settings = recording_settings(&completed, &failed);
        settings["failedRunLimit"] = json!(2);
        settings
    })
    .await;

    let failed = harness.temp.join("failed");
    assert!(failed.join("Dam/Agent/old.mov").exists(), "clip seeding is lazy until Runs is opened");

    let runs: Value = harness.client.get(format!("{API}/api/v1/runs")).send().await.unwrap().json().await.unwrap();
    assert!(!failed.join("Dam/Agent/old.mov").exists());
    assert!(failed.join("Facility/Agent/middle.mov").exists());
    assert!(failed.join("Runway/Agent/newest.mov").exists());
    assert!(failed.join("Surface/Agent/complete.mov").exists());
    let clips = runs["clips"].as_array().unwrap();
    assert_eq!(clips.len(), 3);
    assert_eq!(clips[0]["metadata"]["status"], "complete");
    assert_eq!(clips[1]["metadata"]["status"], "abort");
    assert_eq!(clips[2]["metadata"]["status"], "kia");
}
