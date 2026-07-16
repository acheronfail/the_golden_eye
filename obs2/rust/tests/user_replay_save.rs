mod support;

use std::fs;
use std::path::Path;
use std::time::Duration;

use support::harness::{Harness, probe_duration, recording_settings, wait_for_clip};

/// Count `.mp4` clips written into `dir` (absent dir counts as zero).
fn clip_count(dir: &Path) -> usize {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("mp4"))
                .count()
        })
        .unwrap_or(0)
}

/// Drive one completed run to its stats screen, leaving a save scheduled.
async fn run_to_stats(harness: &Harness, start: &str, complete: &str, stats: &str) {
    let start = harness.frame(start);
    harness.render_until_state(&start, "started").await;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let complete = harness.frame(complete);
    harness.render_until_state(&complete, "complete").await;
    let stats = harness.frame(stats);
    harness.obs.render(stats);
}

/// A user manually saving the replay buffer while no run is finishing must be
/// ignored entirely: the plugin has no save outstanding, so it must not trim the
/// user's file, delete it, or emit a clip of its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn user_initiated_save_while_idle_is_left_untouched() {
    let harness = Harness::start(Duration::ZERO).await;
    let completed = harness.temp.join("completed");
    let failed = harness.temp.join("failed");
    harness.put_settings(recording_settings(&completed, &failed)).await;
    harness.start_monitor().await.error_for_status().unwrap();

    let user_file = harness.obs.user_replay_save();

    // Give any (incorrect) save handling a chance to run before asserting.
    tokio::time::sleep(Duration::from_millis(750)).await;

    assert!(user_file.is_file(), "the user's manually-saved replay file was deleted");
    assert_eq!(clip_count(&completed), 0, "a clip was produced from a user-initiated save");
    assert_eq!(clip_count(&failed), 0, "a clip was produced from a user-initiated save");
    // The plugin never issued a save of its own, so nothing should have run.
    assert_eq!(harness.obs.calls().replay_save, 0);

    harness.stop_monitor().await.error_for_status().unwrap();
}

/// A user save interleaved with a real run must neither be consumed by the run's
/// save nor block it: the user's file survives and the run still yields exactly
/// one correctly-trimmed clip.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn user_save_does_not_disrupt_a_following_run() {
    let harness = Harness::start(Duration::ZERO).await;
    let completed = harness.temp.join("completed");
    let failed = harness.temp.join("failed");
    harness.put_settings(recording_settings(&completed, &failed)).await;
    harness.start_monitor().await.error_for_status().unwrap();

    let user_file = harness.obs.user_replay_save();

    run_to_stats(
        &harness,
        "test/screenshots-av2hdmi/en - start - 03 - Agent.png",
        "test/screenshots-av2hdmi/en - complete - 3 - Secret Agent.png",
        "test/screenshots-av2hdmi/en - stats - 3 - Agent - 0445.png",
    )
    .await;

    let saved = wait_for_clip(&completed).await;
    assert!(probe_duration(&saved) > 0.0);
    assert!(user_file.is_file(), "the user's manual save was consumed by the run's save");
    assert_eq!(clip_count(&completed), 1, "the run should produce exactly one clip");
    // Exactly one plugin-initiated save (the run); the user save is not counted.
    assert_eq!(harness.obs.calls().replay_save, 1);

    harness.stop_monitor().await.error_for_status().unwrap();
}

/// Two runs finishing close together each get their own clip. OBS's saved event
/// has no identity, so without serialization both saves would wake on the same
/// event and trim the same file; a slow async save keeps both in flight to test it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn overlapping_plugin_saves_each_get_their_own_clip() {
    let harness = Harness::start(Duration::ZERO).await;
    let completed = harness.temp.join("completed");
    let failed = harness.temp.join("failed");
    harness.put_settings(recording_settings(&completed, &failed)).await;
    harness.start_monitor().await.error_for_status().unwrap();

    // Make each save take long enough to overlap without adding seconds of wall time.
    harness.obs.set_replay_save_delay(Duration::from_millis(200));

    run_to_stats(
        &harness,
        "test/screenshots-av2hdmi/en - start - 03 - Agent.png",
        "test/screenshots-av2hdmi/en - complete - 3 - Secret Agent.png",
        "test/screenshots-av2hdmi/en - stats - 3 - Agent - 0445.png",
    )
    .await;
    run_to_stats(
        &harness,
        "test/screenshots-av2hdmi/en - start - 03 - Secret Agent.png",
        "test/screenshots-av2hdmi/en - complete - 3 - Secret Agent.png",
        "test/screenshots-av2hdmi/en - stats - 3 - Secret Agent - 0323_1357.png",
    )
    .await;

    // Wait for both clips (the delayed saves are serialized, so the second lands after the first).
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while clip_count(&completed) < 2 {
        assert!(std::time::Instant::now() < deadline, "expected two clips, saw {}", clip_count(&completed));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    assert_eq!(clip_count(&completed), 2, "each run should produce its own clip");
    assert_eq!(harness.obs.calls().replay_save, 2);

    harness.stop_monitor().await.error_for_status().unwrap();
}

/// The ordinary case still cleans up: with only the plugin's own save in the
/// window, the resolved file is unambiguous and its replay source is deleted
/// after trimming.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn normal_run_deletes_its_own_replay_source() {
    let harness = Harness::start(Duration::ZERO).await;
    let completed = harness.temp.join("completed");
    let failed = harness.temp.join("failed");
    harness.put_settings(recording_settings(&completed, &failed)).await;
    harness.start_monitor().await.error_for_status().unwrap();

    run_to_stats(
        &harness,
        "test/screenshots-av2hdmi/en - start - 03 - Agent.png",
        "test/screenshots-av2hdmi/en - complete - 3 - Secret Agent.png",
        "test/screenshots-av2hdmi/en - stats - 3 - Agent - 0445.png",
    )
    .await;

    wait_for_clip(&completed).await;
    assert_eq!(clip_count(&completed), 1);
    // The single replay source we wrote was trimmed and then removed.
    assert_eq!(clip_count(&harness.replay_dir), 0, "the plugin's own replay source should be deleted after trimming");

    harness.stop_monitor().await.error_for_status().unwrap();
}

/// A user manual-save landing *while the plugin's own save is in flight* (so OBS's
/// event may report the user's file) must not be deleted: two files appear and the
/// plugin can't tell them apart, so it still clips but leaves both files on disk.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn concurrent_user_save_during_a_plugin_save_is_not_deleted() {
    let harness = Harness::start(Duration::ZERO).await;
    let completed = harness.temp.join("completed");
    let failed = harness.temp.join("failed");
    harness.put_settings(recording_settings(&completed, &failed)).await;
    harness.start_monitor().await.error_for_status().unwrap();

    // Make the plugin save slow enough to still be in flight when the user saves.
    harness.obs.set_replay_save_delay(Duration::from_millis(750));

    run_to_stats(
        &harness,
        "test/screenshots-av2hdmi/en - start - 03 - Agent.png",
        "test/screenshots-av2hdmi/en - complete - 3 - Secret Agent.png",
        "test/screenshots-av2hdmi/en - stats - 3 - Agent - 0445.png",
    )
    .await;

    // Wait until the plugin has issued its (slow) save, then interleave a user
    // save while it is still waiting for OBS's event.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while harness.obs.calls().replay_save == 0 {
        assert!(std::time::Instant::now() < deadline, "plugin never issued its save");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let user_file = harness.obs.user_replay_save();

    wait_for_clip(&completed).await;

    assert!(user_file.is_file(), "the user's concurrent manual save was deleted");
    assert_eq!(clip_count(&completed), 1, "the run should still produce a clip");
    // Ambiguous window: both replay files (the plugin's and the user's) are kept.
    assert_eq!(clip_count(&harness.replay_dir), 2, "neither replay source should be deleted when ambiguous");

    harness.stop_monitor().await.error_for_status().unwrap();
}
