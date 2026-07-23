mod support;

use std::time::Duration;

use serde_json::{Value, json};
use support::harness::{API, Harness, recording_settings, wait_for_clip};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn failed_run_is_saved_to_the_standard_clip_directory() {
    let harness = Harness::start_with_settings_from_temp(Duration::ZERO, |temp| {
        recording_settings(&temp.join("completed"), &temp.join("failed"))
    })
    .await;
    let completed_dir = harness.temp.join("completed");
    harness.start_monitor().await.error_for_status().unwrap();

    let start = harness.frame("test/screenshots-av2hdmi/en - start - 03 - Secret Agent.png");
    harness.render_until_state(&start, "started").await;

    let failed = harness.frame("test/screenshots-av2hdmi/en - failed - 3 - Secret Agent.png");
    harness.render_until_state(&failed, "failed").await;

    let stats = harness.frame("test/screenshots-av2hdmi/en - stats - 3 - Secret Agent - 0323_1357.png");
    harness.obs.render(stats);
    let saved = wait_for_clip(&completed_dir).await;

    assert!(saved.starts_with(&completed_dir));
    assert!(saved.file_name().unwrap().to_string_lossy().contains("failed"));

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
    assert_eq!(runs["clips"][0]["retentionState"], "pending");
    let run_id = runs["clips"][0]["runId"].as_str().unwrap().to_owned();

    let kept: Value = harness
        .client
        .post(format!("{API}/api/v1/runs/keep"))
        .json(&json!({ "runId": &run_id }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(kept["retentionState"], "kept");

    harness.stop_monitor().await.error_for_status().unwrap();
    harness.start_monitor().await.error_for_status().unwrap();
    let recent: Value = harness
        .client
        .get(format!("{API}/api/v1/runs/recent"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(recent[0]["runId"], run_id);
    assert_eq!(recent[0]["retentionState"], "kept");

    let history: Value = harness
        .client
        .post(format!("{API}/api/v1/runs/delete"))
        .json(&json!({ "runId": &run_id, "keepHistory": true }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(history["path"], "");
    assert!(!saved.exists());

    let deleted: Value = harness
        .client
        .post(format!("{API}/api/v1/runs/delete"))
        .json(&json!({ "runId": &run_id, "keepHistory": false }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(deleted.is_null());
    harness.stop_monitor().await.error_for_status().unwrap();
    let calls = harness.obs.calls();
    assert_eq!(calls.replay_save, 1);
}
