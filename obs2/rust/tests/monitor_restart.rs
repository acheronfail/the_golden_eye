mod support;

use std::time::{Duration, Instant};

use support::harness::{Harness, recording_settings};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn monitor_restart_waits_for_replay_buffer_stop_and_settle() {
    let stop_delay = Duration::from_millis(500);
    let harness = Harness::start(stop_delay).await;
    let mut settings = recording_settings(&harness.temp.join("completed"), &harness.temp.join("failed"));
    settings["stopReplayBufferWhenMonitorStopped"] = true.into();
    harness.put_settings(settings).await;

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
