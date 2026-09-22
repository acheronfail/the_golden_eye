use std::time::{Duration, Instant};

use futures_util::StreamExt;
use opencv::core::{Mat, Size};
use opencv::imgproc;
use opencv::prelude::*;
use serde_json::{Value, json};

use crate::support::harness::{
    API,
    Harness,
    decode_bgra_frames,
    next_app_snapshot,
    snapshot_from_message,
    wait_for_clip,
};
use crate::support::test_obs::Frame;

fn resized(source: &Frame, width: u32, height: u32) -> Frame {
    let pixels = Mat::from_slice(&source.bgra).unwrap();
    let pixels = pixels.reshape(4, source.height as i32).unwrap();
    let mut output = Mat::default();
    imgproc::resize(&pixels, &mut output, Size::new(width as i32, height as i32), 0.0, 0.0, imgproc::INTER_LINEAR)
        .unwrap();
    Frame { width, height, bgra: output.data_bytes().unwrap().to_vec() }
}

async fn start_harness() -> Harness {
    let harness = Harness::start_with_settings_from_temp(Duration::ZERO, |temp| {
        json!({
            "completedOutputPath": temp.join("completed"),
            "recentRunLimit": 5,
            "preRunPaddingSecs": 0,
            "postRunPaddingSecs": 1
        })
    })
    .await;
    harness.start_monitor().await.error_for_status().unwrap();
    harness
}

async fn snapshot(harness: &Harness) -> Value {
    let mut ws = harness.connect_event_stream().await;
    next_app_snapshot(&mut ws, "current capture sequence state").await
}

async fn render_until(harness: &Harness, frame: &Frame, label: &str, accepts: impl Fn(&Value) -> bool) -> Value {
    let mut ws = harness.connect_event_stream().await;
    // Discard the retained snapshot so a previous frame cannot satisfy this step.
    next_app_snapshot(&mut ws, "before sequence step").await;
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last = Value::Null;
    loop {
        harness.obs.render(frame.clone());
        if let Ok(Some(message)) = tokio::time::timeout(Duration::from_millis(120), ws.next()).await
            && let Some(current) = snapshot_from_message(message.unwrap())
        {
            last = current;
            if accepts(&last) {
                return last;
            }
        }
        assert!(Instant::now() < deadline, "{label}: last snapshot {last}");
    }
}

fn phase(snapshot: &Value) -> &Value {
    &snapshot["state"]["monitor"]["wallClocks"]["levelTimerPhase"]
}

async fn missing_frames(harness: &Harness) {
    let before = harness.obs.captures().len();
    for _ in 0..5 {
        harness.obs.render_missing();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let captures = harness.obs.captures();
    assert_eq!(captures.len(), before + 5);
    assert!(captures[before..].iter().all(|capture| capture.source_size.is_none() && capture.output_size.is_none()));
}

async fn runs(harness: &Harness) -> Vec<Value> {
    let result: Value = harness
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
    result["clips"].as_array().unwrap().clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn calibrated_run_survives_source_resize_and_capture_gaps() {
    let harness = start_harness().await;
    let start = harness.frame("frame_tests/screenshots-av2hdmi/en - start - 03 - Agent.png");
    let wide_start = resized(&start, 1280, 720);
    let started = render_until(&harness, &wide_start, "calibrated start", |s| {
        s["state"]["match"]["screen"] == "Start" && s["state"]["recordingState"] == "started"
    })
    .await;
    assert_eq!(started["state"]["match"]["mission"], 1);
    assert_eq!(started["state"]["match"]["part"], 3);
    assert_eq!(started["state"]["match"]["difficulty"], 0);
    let captures = harness.obs.captures();
    assert_eq!(captures[0].source_size, Some((1280, 720)));
    assert_eq!(captures[0].max_height, 480);
    assert_eq!(captures[0].crop, None);
    assert_eq!(captures[0].output_size, Some((853, 480)));
    let visible = Frame { width: 1280, height: 720, bgra: vec![80; 1280 * 720 * 4] };
    render_until(&harness, &visible, "visible frame after calibration", |s| s["state"]["match"]["screen"] == "Unknown")
        .await;
    let captures = harness.obs.captures();
    let calibrated = captures.last().unwrap();
    assert!(calibrated.crop.is_some());
    assert_eq!(calibrated.max_height, 0);
    assert_eq!(calibrated.requested_size, Some((640, 480)));
    assert_eq!(calibrated.output_size, calibrated.requested_size);

    let large_start = resized(&start, 1920, 1080);
    let after_resize = render_until(&harness, &large_start, "start after source resize", |s| {
        s["state"]["match"]["screen"] == "Start" && s["state"]["match"]["part"] == 3
    })
    .await;
    assert_eq!(after_resize["state"]["match"]["difficulty"], 0);
    let resized_capture = harness.obs.captures().last().unwrap().clone();
    assert_eq!(resized_capture.source_size, Some((1920, 1080)));
    assert_eq!(resized_capture.crop, calibrated.crop);
    assert_eq!(resized_capture.output_size, Some((640, 480)));

    missing_frames(&harness).await;
    let gap = snapshot(&harness).await;
    assert_eq!(gap["state"]["recordingState"], "started");
    assert_eq!(phase(&gap), "awaitingInitialBlack");
    assert_eq!(harness.obs.calls().replay_save, 0);
    assert!(runs(&harness).await.is_empty());

    let black = Frame { width: 1920, height: 1080, bgra: vec![0; 1920 * 1080 * 4] };
    render_until(&harness, &black, "actual black frame", |s| phase(s) == "awaitingFirstCutscene").await;
    let cutscene =
        harness.frame("frame_tests/screenshots-rt4kce/jp - unknown - fade-2-first-to-second - before-black.png");
    render_until(&harness, &resized(&cutscene, 1920, 1080), "visible cutscene", |s| {
        phase(s) == "awaitingFirstCutsceneFade"
    })
    .await;
    missing_frames(&harness).await;
    assert_eq!(phase(&snapshot(&harness).await), "awaitingFirstCutsceneFade");
    assert_eq!(harness.obs.calls().replay_save, 0);

    let complete = harness.frame("frame_tests/screenshots-av2hdmi/en - complete - 3 - Secret Agent.png");
    render_until(&harness, &resized(&complete, 1920, 1080), "completion", |s| {
        s["state"]["recordingState"] == "complete"
    })
    .await;
    let stats = harness.frame("frame_tests/screenshots-av2hdmi/en - stats - 3 - Agent - 0445.png");
    render_until(&harness, &resized(&stats, 1920, 1080), "stats", |s| s["state"]["recordingState"] == "savePending")
        .await;
    missing_frames(&harness).await;
    wait_for_clip(&harness.temp.join("completed")).await;
    let saved = runs(&harness).await;
    assert_eq!(saved.len(), 1);
    let metadata = &saved[0]["metadata"];
    assert_eq!(metadata["levelNumber"], 3);
    assert_eq!(metadata["difficulty"], "Agent");
    assert_eq!(metadata["status"], "complete");
    assert_eq!(metadata["timeSeconds"], 285);
    assert_eq!(harness.obs.calls().replay_save, 1);

    let next = harness.frame("frame_tests/screenshots-av2hdmi/en - start - 01 - Agent.png");
    render_until(&harness, &resized(&next, 1280, 720), "next run", |s| {
        s["state"]["recordingState"] == "started" && s["state"]["match"]["part"] == 1
    })
    .await;
    missing_frames(&harness).await;
    assert_eq!(harness.obs.calls().replay_save, 1);
    assert_eq!(runs(&harness).await.len(), 1);
    harness.stop_monitor().await.error_for_status().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn recorded_run_survives_missing_frames_and_resolution_changes() {
    let harness = start_harness().await;
    let frames = decode_bgra_frames(&harness.root.join("frame_tests/clips/rt4kce-completed.mp4"));
    assert!(frames.len() > 20);
    let original_size = (frames[0].width, frames[0].height);
    let enlarged_size = (original_size.0 * 2, original_size.1 * 2);
    let count = frames.len();
    for (index, frame) in frames.into_iter().enumerate() {
        if index % 7 == 3 {
            harness.obs.render_missing();
        } else if (count / 3..count * 2 / 3).contains(&index) {
            harness.obs.render(resized(&frame, enlarged_size.0, enlarged_size.1));
        } else {
            harness.obs.render(frame);
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    wait_for_clip(&harness.temp.join("completed")).await;
    let saved = runs(&harness).await;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0]["metadata"]["status"], "complete");
    assert_eq!(saved[0]["metadata"]["levelNumber"], 3);
    assert_eq!(saved[0]["metadata"]["difficulty"], "Agent");
    assert_eq!(saved[0]["metadata"]["timeSeconds"], 28);
    assert_eq!(harness.obs.calls().replay_save, 1);
    let captures = harness.obs.captures();
    assert_eq!(captures.len(), count);
    assert!(captures.iter().any(|c| c.source_size == Some(original_size)));
    assert!(captures.iter().any(|c| c.source_size == Some(enlarged_size)));
    assert!(captures.iter().any(|c| c.source_size.is_none() && c.output_size.is_none()));
    assert!(captures.iter().filter_map(|c| c.output_size).all(|(_, height)| height <= 480));
    harness.stop_monitor().await.error_for_status().unwrap();
}
