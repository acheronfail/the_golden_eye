mod support;

use std::time::Duration;

use serde_json::{Value, json};
use support::harness::{API, Harness, decode_bgra_frames, wait_for_clip};

/// Replaying `test/clips/kia.mp4` through the monitor must record the run's real
/// time (14s), not the `6:14` misread the stats overlay shows on its first frame.
/// Guards the state-machine voting in `recording::RecordingState::on_frame`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn first_stats_frame_misread_does_not_poison_the_saved_run_time() {
    let harness = Harness::start(Duration::ZERO).await;
    let completed_dir = harness.temp.join("completed");
    let failed_dir = harness.temp.join("failed");

    // Give the pending save a padding window so the vote across the first few
    // stats frames settles before the clip is written.
    harness
        .put_settings(json!({
            "completedOutputPath": completed_dir,
            "failedOutputPath": failed_dir,
            "saveFailedRuns": true,
            "minimumFailedRunLengthSecs": 0,
            "failedRunLimit": 0,
            "clipFilenameTemplate": "stats-{status}-{time}",
            "preRunPaddingSecs": 0,
            "postRunPaddingSecs": 1,
            "discordNotificationsEnabled": false
        }))
        .await;
    harness.start_monitor().await.error_for_status().unwrap();

    // Render every frame twice so the misread first stats frame spans several
    // matched frames, as it can live. Pace renders so the capacity-1 mailbox keeps
    // up -- the misread frame must be matched more than once to bite.
    let frames = decode_bgra_frames(&harness.root.join("test/clips/kia.mp4"));
    assert!(frames.len() > 1, "expected a multi-frame clip");
    for frame in frames {
        for _ in 0..2 {
            harness.obs.render(frame.clone());
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    }

    // Frames have now stopped (as if the source were paused), and the monitor is
    // still running: the save must still fire on its own padding timer rather than
    // waiting for the monitor to stop. The KIA run lands in the failed directory.
    let saved = wait_for_clip(&failed_dir).await;
    assert!(saved.starts_with(&failed_dir));

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
    let clips = runs["clips"].as_array().expect("clips array");
    assert_eq!(clips.len(), 1, "expected exactly one saved run");
    assert_eq!(clips[0]["metadata"]["status"], "kia");
    assert_eq!(
        clips[0]["metadata"]["timeSeconds"], 14,
        "saved run time should be the corrected 14s, not the first-frame misread"
    );

    harness.stop_monitor().await.error_for_status().unwrap();
}

/// The minimum-failed-run-length gate must use the corrected time too: with a
/// 100s minimum, the KIA run (real 14s) is discarded despite the 374s first-frame
/// misread. Guards the deferred gate in `RecordingState::take_pending_job`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn minimum_failed_run_length_gate_uses_the_corrected_time() {
    let harness = Harness::start(Duration::ZERO).await;
    let completed_dir = harness.temp.join("completed");
    let failed_dir = harness.temp.join("failed");

    harness
        .put_settings(json!({
            "completedOutputPath": completed_dir,
            "failedOutputPath": failed_dir,
            "saveFailedRuns": true,
            // Longer than the real 14s but shorter than the 374s misread.
            "minimumFailedRunLengthSecs": 100,
            "failedRunLimit": 0,
            "clipFilenameTemplate": "stats-{status}-{time}",
            "preRunPaddingSecs": 0,
            "postRunPaddingSecs": 1,
            "discordNotificationsEnabled": false
        }))
        .await;
    harness.start_monitor().await.error_for_status().unwrap();

    let frames = decode_bgra_frames(&harness.root.join("test/clips/kia.mp4"));
    for frame in frames {
        harness.obs.render(frame);
        tokio::time::sleep(Duration::from_millis(40)).await;
    }

    // Shutdown flushes any pending save synchronously, so the discard decision is
    // final once stop returns.
    harness.stop_monitor().await.error_for_status().unwrap();

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
    assert_eq!(
        runs["clips"].as_array().expect("clips array").len(),
        0,
        "run shorter than the minimum (by its corrected time) must not be saved"
    );
    // The gate short-circuits before OBS is ever asked to save the buffer.
    assert_eq!(harness.obs.calls().replay_save, 0, "no replay save should have been requested");
}
