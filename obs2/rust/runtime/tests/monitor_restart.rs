use std::time::{Duration, Instant};

use crate::support::harness::{Harness, recording_settings};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn monitor_restart_waits_for_replay_buffer_stop_and_settle() {
    let stop_delay = Duration::from_millis(500);
    let harness = Harness::start_with_settings_from_temp(stop_delay, |temp| {
        let mut settings = recording_settings(&temp.join("completed"), &temp.join("failed"));
        settings["stopReplayBufferWhenMonitorStopped"] = true.into();
        settings
    })
    .await;

    harness.start_monitor().await.error_for_status().unwrap();
    assert!(harness.obs.replay_active());
    assert!(harness.wait_for_status(|status| status["enabled"] == true).await["enabled"] == true);

    harness.stop_monitor().await.error_for_status().unwrap();
    assert_eq!(harness.obs.calls().replay_stop, 1);
    assert!(harness.obs.replay_active(), "the delayed test OBS should still be stopping");

    let restart_requested_at = Instant::now();
    harness.start_monitor().await.error_for_status().unwrap();
    let restart_elapsed = restart_requested_at.elapsed();

    // The production path waits for the 500 ms asynchronous stop and then its
    // 400 ms post-stopped settle delay before asking OBS to start again.
    assert!(restart_elapsed >= Duration::from_millis(850), "monitor restarted too early after {restart_elapsed:?}");
    assert!(harness.obs.replay_active());
    assert!(harness.wait_for_status(|status| status["enabled"] == true).await["enabled"] == true);

    let calls = harness.obs.calls();
    assert_eq!(calls.replay_start, 2);
    assert_eq!(calls.replay_stop, 1);
    assert_eq!(calls.frame_callback_register, 2);
    assert_eq!(calls.frame_callback_unregister, 1);
    assert_eq!(calls.capture_create, 2);
    assert_eq!(calls.capture_destroy, 1);

    harness.stop_monitor().await.error_for_status().unwrap();
    harness.wait_for_replay_inactive().await;
    let calls = harness.obs.calls();
    assert_eq!(calls.replay_stop, 2);
    assert_eq!(calls.frame_callback_unregister, 2);
    assert_eq!(calls.capture_destroy, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn monitor_start_is_idempotent_and_rejects_conflicting_sources() {
    use reqwest::StatusCode;

    use crate::support::harness::API;

    let harness = Harness::start(Duration::ZERO).await;
    let invalid = harness
        .client
        .post(format!("{API}/api/v1/monitor/start"))
        .json(&serde_json::json!({"sourceName": "invalid\0source"}))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    assert_eq!(invalid.text().await.unwrap(), "source name contains a null byte");
    assert_eq!(harness.obs.calls().capture_create, 0);

    harness.start_monitor().await.error_for_status().unwrap();
    harness.start_monitor().await.error_for_status().unwrap();
    let conflict = harness
        .client
        .post(format!("{API}/api/v1/monitor/start"))
        .json(&serde_json::json!({"sourceName": "Another Capture"}))
        .send()
        .await
        .unwrap();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    assert_eq!(conflict.text().await.unwrap(), "a monitor is already running");
    assert_eq!(harness.obs.calls().capture_create, 1);
    assert_eq!(harness.obs.calls().frame_callback_register, 1);

    harness.stop_monitor().await.error_for_status().unwrap();
    assert_eq!(harness.stop_monitor().await.status(), StatusCode::CONFLICT);
    assert_eq!(harness.obs.calls().capture_destroy, 1);
    assert_eq!(harness.obs.calls().frame_callback_unregister, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn slow_replay_start_uses_one_request_and_keeps_http_responsive() {
    use crate::support::harness::API;

    let harness = Harness::start(Duration::ZERO).await;
    harness.obs.set_replay_start_behavior(Duration::from_millis(2500), None);
    let start = Instant::now();
    let (response, ()) = tokio::join!(harness.start_monitor(), async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let response =
            harness.client.get(format!("{API}/api/v1/sources")).timeout(Duration::from_secs(1)).send().await.unwrap();
        response.error_for_status().unwrap();
    });
    response.error_for_status().unwrap();
    assert!(start.elapsed() >= Duration::from_millis(2500));
    assert_eq!(harness.obs.calls().replay_start, 1);
    assert!(harness.obs.replay_active());
    harness.stop_monitor().await.error_for_status().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn repeated_rapid_restarts_issue_one_request_per_start() {
    let harness = Harness::start_with_settings(
        Duration::from_millis(100),
        serde_json::json!({"stopReplayBufferWhenMonitorStopped": true}),
    )
    .await;
    harness.obs.set_replay_start_behavior(Duration::from_millis(50), None);
    for expected_starts in 1..=4 {
        harness.start_monitor().await.error_for_status().unwrap();
        assert!(harness.obs.replay_active());
        assert_eq!(harness.obs.calls().replay_start, expected_starts);
        harness.stop_monitor().await.error_for_status().unwrap();
    }
    harness.wait_for_replay_inactive().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn missing_replay_directory_fails_before_asking_obs_to_start() {
    let harness = Harness::start_with_settings_from_temp(Duration::ZERO, |temp| {
        recording_settings(&temp.join("completed"), &temp.join("failed"))
    })
    .await;
    std::fs::remove_dir(&harness.replay_dir).unwrap();
    let response = harness.start_monitor().await;
    assert_eq!(response.status(), reqwest::StatusCode::PRECONDITION_FAILED);
    let message = response.text().await.unwrap();
    assert!(message.contains("recording output directory"), "{message}");
    assert!(message.contains(harness.replay_dir.to_str().unwrap()), "{message}");
    assert_eq!(harness.obs.calls().replay_start, 0);
    assert_eq!(harness.obs.calls().capture_create, 0);
    std::fs::create_dir(&harness.replay_dir).unwrap();
    harness.start_monitor().await.error_for_status().unwrap();
    harness.stop_monitor().await.error_for_status().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn replay_start_failure_reports_obs_error_without_retrying() {
    let harness = Harness::start(Duration::ZERO).await;
    harness.obs.set_replay_start_behavior(Duration::from_millis(30), Some("Encoder initialization failed".into()));
    let start = Instant::now();
    let response = harness.start_monitor().await;
    assert_eq!(response.status(), reqwest::StatusCode::PRECONDITION_FAILED);
    assert!(response.text().await.unwrap().contains("Encoder initialization failed"));
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_eq!(harness.obs.calls().replay_start, 1);
    assert_eq!(harness.obs.calls().capture_create, 0);
    harness.obs.set_replay_start_behavior(Duration::ZERO, None);
    harness.start_monitor().await.error_for_status().unwrap();
    harness.stop_monitor().await.error_for_status().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn replay_start_timeout_reports_actionable_error_without_retrying() {
    let harness = Harness::start(Duration::ZERO).await;
    harness.obs.ignore_replay_start();
    let start = Instant::now();
    let response = harness.start_monitor().await;
    assert_eq!(response.status(), reqwest::StatusCode::PRECONDITION_FAILED);
    assert!(response.text().await.unwrap().contains("Check OBS for an error"));
    assert!(start.elapsed() >= Duration::from_secs(10));
    assert_eq!(harness.obs.calls().replay_start, 1);
    assert_eq!(harness.obs.calls().capture_create, 0);
}
