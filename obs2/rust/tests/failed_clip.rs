mod support;

use std::time::Duration;

use serde_json::Value;
use support::harness::{API, Harness, recording_settings, wait_for_clip};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn failed_run_is_saved_to_the_configured_failed_directory() {
    let harness = Harness::start_with_settings_from_temp(Duration::ZERO, |temp| {
        recording_settings(&temp.join("completed"), &temp.join("failed"))
    })
    .await;
    let completed_dir = harness.temp.join("completed");
    let failed_dir = harness.temp.join("failed");
    harness.start_monitor().await.error_for_status().unwrap();

    let start = harness.frame("test/screenshots-av2hdmi/en - start - 03 - Secret Agent.png");
    harness.render_until_state(&start, "started").await;

    let failed = harness.frame("test/screenshots-av2hdmi/en - failed - 3 - Secret Agent.png");
    harness.render_until_state(&failed, "failed").await;

    let stats = harness.frame("test/screenshots-av2hdmi/en - stats - 3 - Secret Agent - 0323_1357.png");
    harness.obs.render(stats);
    let saved = wait_for_clip(&failed_dir).await;

    assert!(saved.starts_with(&failed_dir));
    assert!(saved.file_name().unwrap().to_string_lossy().contains("failed"));
    assert!(!completed_dir.exists() || completed_dir.read_dir().unwrap().next().is_none());

    let runs: Value = harness
        .client
        .get(format!("{API}/api/v1/runs"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(runs["clips"].as_array().unwrap().len(), 1);
    assert_eq!(runs["clips"][0]["metadata"]["status"], "failed");
    assert_eq!(runs["clips"][0]["metadata"]["difficulty"], "Secret Agent");

    harness.stop_monitor().await.error_for_status().unwrap();
    let calls = harness.obs.calls();
    assert_eq!(calls.replay_save, 1);
}
