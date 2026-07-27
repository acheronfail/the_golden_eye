use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde_json::Value;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::support::harness::{Harness, SOURCE_NAME, next_app_snapshot, snapshot_from_message};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn monitor_snapshot_tracks_start_match_and_stop() {
    let harness = Harness::start(Duration::ZERO).await;
    let mut ws = harness.connect_event_stream().await;

    let initial = next_app_snapshot(&mut ws, "initial snapshot").await;
    assert_eq!(initial["state"]["monitor"]["enabled"], false);
    assert_eq!(initial["state"]["monitor"]["wallClocks"]["sessionRunning"], false);
    assert!(initial["state"]["match"].is_null());
    assert!(initial["state"]["recordingState"].is_null());

    harness.start_monitor().await.error_for_status().unwrap();
    let started =
        wait_for_snapshot(&mut ws, "monitor enabled", |snapshot| snapshot["state"]["monitor"]["enabled"] == true).await;
    assert_eq!(started["state"]["monitor"]["sourceName"], SOURCE_NAME);
    assert_eq!(started["state"]["monitor"]["wallClocks"]["sessionRunning"], true);
    assert!(started["state"]["monitor"]["wallClocks"]["sessionStartedAtUnixMs"].is_number());
    assert!(started["state"]["match"].is_null());

    let frame = harness.frame("test/screenshots-av2hdmi/en - start - 03 - Agent.png");
    let matched = render_until_snapshot(&harness, &mut ws, &frame, "start-screen match", |snapshot| {
        snapshot["state"]["match"]["screen"] == "Start"
    })
    .await;
    assert_eq!(matched["state"]["monitor"]["enabled"], true);
    assert_eq!(matched["state"]["match"]["mission"], 1);
    assert_eq!(matched["state"]["match"]["part"], 3);
    assert_eq!(matched["state"]["match"]["difficulty"], 0);
    assert_eq!(matched["state"]["monitor"]["wallClocks"]["levelRunning"], false);
    assert_eq!(matched["state"]["monitor"]["wallClocks"]["levelElapsedMs"], 0);
    assert_eq!(matched["state"]["monitor"]["wallClocks"]["levelTimerPhase"], "awaitingInitialBlack");
    assert_eq!(matched["state"]["monitor"]["wallClocks"]["introSwirlDelayMs"], 4_383);

    let initial_black = harness.frame("test/screenshots-rt4kce/jp - unknown - fade-1-load-first-cutscene - black.png");
    render_until_snapshot(&harness, &mut ws, &initial_black, "initial loading black", |snapshot| {
        snapshot["state"]["monitor"]["wallClocks"]["levelTimerPhase"] == "awaitingFirstCutscene"
    })
    .await;
    let first_cutscene =
        harness.frame("test/screenshots-rt4kce/jp - unknown - fade-2-first-to-second - before-black.png");
    render_until_snapshot(&harness, &mut ws, &first_cutscene, "first cutscene visible", |snapshot| {
        snapshot["state"]["monitor"]["wallClocks"]["levelTimerPhase"] == "awaitingFirstCutsceneFade"
    })
    .await;
    let first_fade = harness.frame("test/screenshots-rt4kce/jp - unknown - fade-2-first-to-second - black.png");
    render_until_snapshot(&harness, &mut ws, &first_fade, "first cutscene fade", |snapshot| {
        snapshot["state"]["monitor"]["wallClocks"]["levelTimerPhase"] == "awaitingSecondFadeOrSwirl"
    })
    .await;
    let second_cutscene =
        harness.frame("test/screenshots-rt4kce/jp - unknown - fade-3-second-to-gameplay - before-black.png");
    render_until_snapshot(&harness, &mut ws, &second_cutscene, "second cutscene visible", |snapshot| {
        snapshot["state"]["monitor"]["wallClocks"]["levelTimerPhase"] == "awaitingSecondFadeOrSwirl"
            && snapshot["state"]["monitor"]["wallClocks"]["fadeDetection"]["detected"] == false
    })
    .await;
    let second_fade = harness.frame("test/screenshots-rt4kce/jp - unknown - fade-3-second-to-gameplay - black.png");
    render_until_snapshot(&harness, &mut ws, &second_fade, "skipped swirl fade", |snapshot| {
        snapshot["state"]["monitor"]["wallClocks"]["levelTimerPhase"] == "awaitingGameplayAfterSkip"
    })
    .await;
    let gameplay = harness.frame("test/screenshots-rt4kce/jp - unknown - fade-3-second-to-gameplay - after-black.png");
    let running = render_until_snapshot(&harness, &mut ws, &gameplay, "level timer running", |snapshot| {
        snapshot["state"]["monitor"]["wallClocks"]["levelRunning"] == true
    })
    .await;
    assert!(running["state"]["monitor"]["wallClocks"]["levelStartedAtUnixMs"].is_number());
    assert!(running["state"]["monitor"]["wallClocks"]["levelElapsedMs"].as_u64().unwrap() >= 200);
    assert_eq!(running["state"]["monitor"]["wallClocks"]["levelStartReason"], "fade");

    let mut reloaded_ws = harness.connect_event_stream().await;
    let reloaded = next_app_snapshot(&mut reloaded_ws, "reloaded browser snapshot").await;
    assert_eq!(reloaded["state"]["monitor"]["wallClocks"], running["state"]["monitor"]["wallClocks"]);

    harness.stop_monitor().await.error_for_status().unwrap();
    let stopped = wait_for_snapshot(&mut ws, "monitor stopped snapshot", |snapshot| {
        snapshot["state"]["monitor"]["enabled"] == false && snapshot["state"]["match"].is_null()
    })
    .await;
    assert!(stopped["state"]["recordingState"].is_null());
    assert_eq!(stopped["state"]["monitor"]["wallClocks"]["sessionRunning"], false);
    assert_eq!(stopped["state"]["monitor"]["wallClocks"]["levelRunning"], false);
    assert!(stopped["state"]["monitor"]["wallClocks"]["sessionElapsedMs"].as_u64().unwrap() > 0);
}

async fn wait_for_snapshot(
    ws: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    label: &str,
    predicate: impl Fn(&Value) -> bool,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = next_app_snapshot(ws, label).await;
        if predicate(&snapshot) {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {label}; last snapshot: {snapshot}");
    }
}

async fn render_until_snapshot(
    harness: &Harness,
    ws: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    frame: &crate::support::test_obs::Frame,
    label: &str,
    predicate: impl Fn(&Value) -> bool,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last = Value::Null;
    loop {
        harness.obs.render(frame.clone());
        match tokio::time::timeout(Duration::from_millis(120), ws.next()).await {
            Ok(Some(Ok(message))) => {
                if let Some(snapshot) = snapshot_from_message(message) {
                    last = snapshot.clone();
                    if predicate(&snapshot) {
                        return snapshot;
                    }
                }
            }
            Ok(Some(Err(err))) => panic!("app event stream failed while waiting for {label}: {err}"),
            Ok(None) => panic!("app event stream ended while waiting for {label}"),
            Err(_) => {}
        }
        assert!(Instant::now() < deadline, "timed out waiting for {label}; last snapshot: {last}");
    }
}
