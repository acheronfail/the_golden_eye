mod support;

use std::time::Duration;

use serde_json::{Value, json};
use support::harness::{API, Harness, SOURCE_NAME, probe_duration, recording_settings, visual_second, wait_for_clip};

unsafe extern "C" fn queued_test_task(param: *mut std::ffi::c_void) {
    // SAFETY: the test passes a valid mutable bool for the synchronous callback.
    unsafe { *(param.cast::<bool>()) = true };
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn obs_apis_and_completed_run_save_the_correct_replay_window() {
    let harness = Harness::start_with_settings_from_temp(Duration::ZERO, |temp| {
        recording_settings(&temp.join("clips"), &temp.join("failed"))
    })
    .await;
    let fixture = harness.root.join("test/clips/replay-buffer-60s.mp4");
    let clips_dir = harness.temp.join("clips");

    assert_eq!(probe_duration(&fixture), 60.0);
    assert_eq!(visual_second(&fixture, 34.2), 34, "fixture visual timestamp should be machine-readable");

    ge_rust::ge_browser_dock_post_load();
    assert!(harness.obs.dock_json().contains("thegoldeneyedashboard"));

    let replay: Value =
        harness.client.get(format!("{API}/api/v1/replay-buffer/status")).send().await.unwrap().json().await.unwrap();
    assert_eq!(replay["enabled"], true);
    assert_eq!(replay["available"], true);
    assert_eq!(replay["active"], false);
    assert_eq!(replay["maxSeconds"], 60);
    assert_eq!(replay["outputDirectory"], harness.replay_dir.to_string_lossy().as_ref());

    let sources: Value =
        harness.client.get(format!("{API}/api/v1/sources")).send().await.unwrap().json().await.unwrap();
    assert_eq!(sources, json!([{"name":SOURCE_NAME,"id":"test_input"}]));
    harness.obs.set_sources(vec![("Renamed Capture".into(), "test_input".into())]);
    ge_rust::ge_sources_changed();
    let sources: Value =
        harness.client.get(format!("{API}/api/v1/sources")).send().await.unwrap().json().await.unwrap();
    assert_eq!(sources[0]["name"], "Renamed Capture");
    harness.obs.set_sources(vec![(SOURCE_NAME.into(), "test_input".into())]);
    ge_rust::ge_sources_changed();

    let start = harness.frame("test/screenshots-av2hdmi/en - start - 03 - Agent.png");
    harness.obs.set_frame(start.clone());
    let screenshot =
        harness.client.get(format!("{API}/api/v1/screenshot?source=GoldenEye%20Capture")).send().await.unwrap();
    assert!(screenshot.status().is_success());
    assert_eq!(screenshot.headers()[reqwest::header::CONTENT_TYPE], "image/bmp");

    let matched: Value = harness
        .client
        .post(format!("{API}/api/v1/match?source=GoldenEye%20Capture&lang=en"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(matched["match"]["screen"], "Start");

    harness.client.post(format!("{API}/api/v1/record/start")).send().await.unwrap().error_for_status().unwrap();
    harness.client.post(format!("{API}/api/v1/record/stop")).send().await.unwrap().error_for_status().unwrap();

    harness.start_monitor().await.error_for_status().unwrap();
    assert!(harness.obs.replay_active());

    harness.render_until_state(&start, "started").await;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let complete = harness.frame("test/screenshots-av2hdmi/en - complete - 3 - Secret Agent.png");
    harness.render_until_state(&complete, "complete").await;
    let stats = harness.frame("test/screenshots-av2hdmi/en - stats - 3 - Agent - 0445.png");
    harness.obs.render(stats);

    let saved = wait_for_clip(&clips_dir).await;
    let duration = probe_duration(&saved);
    assert!((1.0..=3.5).contains(&duration), "unexpected trimmed duration {duration}");
    let first_visual_second = visual_second(&saved, 0.2);
    let last_visual_second = visual_second(&saved, (duration - 0.2).max(0.0));
    assert!((56..=59).contains(&first_visual_second), "trim began at source second {first_visual_second}");
    assert_eq!(last_visual_second, 59, "trim should end at the replay save point");

    harness.stop_monitor().await.error_for_status().unwrap();
    support::test_obs::obs_frontend_replay_buffer_stop();

    let mut queued = false;
    // SAFETY: the test host runs the callback synchronously and queued lives through the call.
    unsafe { support::test_obs::obs_queue_task(0, queued_test_task, (&mut queued as *mut bool).cast(), false) };
    assert!(queued);

    let calls = harness.obs.calls();
    assert_eq!(calls.recording_start, 1);
    assert_eq!(calls.recording_stop, 1);
    assert_eq!(calls.replay_start, 1);
    assert_eq!(calls.replay_save, 1);
    assert_eq!(calls.replay_stop, 1);
    assert!(calls.capture_create >= 1);
    assert!(calls.capture_get_frame >= 3);
    assert_eq!(calls.capture_destroy, calls.capture_create);
    assert_eq!(calls.frame_callback_register, 1);
    assert_eq!(calls.frame_callback_unregister, 1);
    assert_eq!(calls.dock_config_save, 1);
    assert_eq!(calls.queue_task, 1);
}
