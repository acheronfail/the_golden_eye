use std::time::{Duration, Instant};

use crate::support::harness::{API, Harness, output_clip, recording_settings};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn runtime_shutdown_cancels_an_in_flight_replay_and_preserves_the_raw_file() {
    let harness = Harness::start_with_settings_from_temp(Duration::ZERO, |temp| {
        recording_settings(&temp.join("clips"), &temp.join("failed"))
    })
    .await;
    harness.obs.defer_replay_saves();
    harness.start_monitor().await.error_for_status().unwrap();
    let start = harness.frame("frame_tests/screenshots-av2hdmi/en - start - 03 - Agent.png");
    harness.render_until_state(&start, "started").await;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let complete = harness.frame("frame_tests/screenshots-av2hdmi/en - complete - 3 - Secret Agent.png");
    harness.render_until_state(&complete, "complete").await;
    let stats = harness.frame("frame_tests/screenshots-av2hdmi/en - stats - 3 - Agent - 0445.png");
    harness.render_until_state(&stats, "savePending").await;
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness.obs.deferred_replay_save_count() == 0 {
        harness.obs.render(stats.clone());
        assert!(Instant::now() < deadline, "save worker did not request a replay");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    harness.stop_monitor().await.error_for_status().unwrap();
    assert_eq!(harness.obs.calls().frame_callback_unregister, 1);

    stage_update(&harness);
    let apply = harness.client.post(format!("{API}/api/v1/updates/apply")).send().await.unwrap();
    assert_eq!(apply.status().as_u16(), 409, "outstanding saves must prevent reload even after monitoring stops");

    let (stopped_tx, stopped_rx) = std::sync::mpsc::channel();
    let shutdown = std::thread::spawn(move || {
        ge_runtime::ge_runtime_begin_shutdown();
        ge_runtime::ge_runtime_stop();
        stopped_tx.send(()).unwrap();
    });
    stopped_rx.recv_timeout(Duration::from_secs(5)).expect("shutdown must not need a replay completion callback");
    shutdown.join().unwrap();
    assert!(output_clip(&harness.temp.join("clips")).is_none());
    assert_eq!(std::fs::read_dir(&harness.replay_dir).unwrap().count(), 1, "raw replay must survive cancellation");
    harness.obs.finish_deferred_replay_saves();
    assert_eq!(std::fs::read_dir(&harness.replay_dir).unwrap().count(), 1);
}

fn stage_update(harness: &Harness) {
    let staged = harness.temp.join(".ge_update_staged");
    std::fs::create_dir_all(&staged).unwrap();
    std::fs::write(staged.join("golden_core.test"), b"test update").unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn queued_picker_blocks_updates_but_not_runtime_shutdown() {
    let harness = Harness::start(Duration::ZERO).await;
    harness.obs.defer_ui_tasks();
    let client = harness.client.clone();
    let request = tokio::spawn(async move {
        client.post(format!("{API}/api/v1/folders/pick")).json(&serde_json::json!({})).send().await
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while harness.obs.calls().queue_task == 0 {
        assert!(Instant::now() < deadline, "picker was not queued");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    stage_update(&harness);
    let apply = harness.client.post(format!("{API}/api/v1/updates/apply")).send().await.unwrap();
    assert_eq!(apply.status().as_u16(), 409);
    let (stopped_tx, stopped_rx) = std::sync::mpsc::channel();
    let shutdown = std::thread::spawn(move || {
        ge_runtime::ge_runtime_stop();
        stopped_tx.send(()).unwrap();
    });
    stopped_rx.recv_timeout(Duration::from_secs(5)).expect("queued UI work must not block runtime shutdown");
    shutdown.join().unwrap();
    // Model OBS draining its UI queue after Rust has stopped; no dialog may open.
    harness.obs.finish_deferred_ui_tasks();
    let _ = request.await;
}
