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
    // Give the pending save a padding window so the vote across the first few
    // stats frames settles before the clip is written.
    let harness = Harness::start_with_settings_from_temp(Duration::ZERO, |temp| {
        json!({
            "completedOutputPath": temp.join("completed"),
            "failedOutputPath": temp.join("failed"),
            "saveFailedRuns": true,
            "minimumFailedRunLengthSecs": 0,
            "failedRunLimit": 0,
            "clipFilenameTemplate": "stats-{status}-{time}",
            "preRunPaddingSecs": 0,
            "postRunPaddingSecs": 1
        })
    })
    .await;
    let failed_dir = harness.temp.join("failed");
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

/// Replaying a real RT4K capture-card clip (`test/clips/rt4kce-completed.mp4`,
/// a completed Runway run whose stats overlay reads 0:28 / 5:00 / best 0:28)
/// through the whole native pipeline must record the run as complete with its
/// real 28s time. Guards the variance-weighted digit reader (which distinguishes
/// the look-alike stats glyphs) plus the per-field stats vote, end to end.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn rt4k_completed_run_records_the_correct_stats_time() {
    let harness = Harness::start_with_settings_from_temp(Duration::ZERO, |temp| {
        json!({
            "completedOutputPath": temp.join("completed"),
            "failedOutputPath": temp.join("failed"),
            "saveFailedRuns": true,
            "minimumFailedRunLengthSecs": 0,
            "failedRunLimit": 0,
            "clipFilenameTemplate": "stats-{status}-{time}",
            "preRunPaddingSecs": 0,
            "postRunPaddingSecs": 1
        })
    })
    .await;
    let completed_dir = harness.temp.join("completed");
    harness.start_monitor().await.error_for_status().unwrap();

    // Real capture-card frames, English overlay. The clip leads with a start
    // screen so the monitor auto-detects the ROM language, then runs complete ->
    // stats -> select, scheduling a completed save off the stats screen; pace
    // renders so the capacity-1 mailbox keeps up.
    let frames = decode_bgra_frames(&harness.root.join("test/clips/rt4kce-completed.mp4"));
    assert!(frames.len() > 1, "expected a multi-frame clip");
    for frame in frames {
        harness.obs.render(frame);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let saved = wait_for_clip(&completed_dir).await;
    assert!(saved.starts_with(&completed_dir), "a completed run lands in the completed directory");

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
    assert_eq!(clips[0]["metadata"]["status"], "complete");
    assert_eq!(
        clips[0]["metadata"]["timeSeconds"], 28,
        "saved run time should be the 28s read off the RT4K stats overlay"
    );

    harness.stop_monitor().await.error_for_status().unwrap();
}

/// The minimum-failed-run-length gate must use the corrected time too: with a
/// 100s minimum, the KIA run (real 14s) is discarded despite the 374s first-frame
/// misread. Guards the deferred gate in `RecordingState::take_pending_job`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn minimum_failed_run_length_gate_uses_the_corrected_time() {
    let harness = Harness::start_with_settings_from_temp(Duration::ZERO, |temp| {
        json!({
            "completedOutputPath": temp.join("completed"),
            "failedOutputPath": temp.join("failed"),
            "saveFailedRuns": true,
            // Longer than the real 14s but shorter than the 374s misread.
            "minimumFailedRunLengthSecs": 100,
            "failedRunLimit": 0,
            "clipFilenameTemplate": "stats-{status}-{time}",
            "preRunPaddingSecs": 0,
            "postRunPaddingSecs": 1
        })
    })
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
