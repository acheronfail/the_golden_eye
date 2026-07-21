mod support;

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde_json::Value;
use support::harness::{Harness, SOURCE_NAME, next_app_snapshot, snapshot_from_message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn monitor_snapshot_tracks_start_match_and_stop() {
    let harness = Harness::start(Duration::ZERO).await;
    let mut ws = harness.connect_event_stream().await;

    let initial = next_app_snapshot(&mut ws, "initial snapshot").await;
    assert_eq!(initial["state"]["monitor"]["enabled"], false);
    assert!(initial["state"]["match"].is_null());
    assert!(initial["state"]["recordingState"].is_null());

    harness.start_monitor().await.error_for_status().unwrap();
    let started =
        wait_for_snapshot(&mut ws, "monitor enabled", |snapshot| snapshot["state"]["monitor"]["enabled"] == true).await;
    assert_eq!(started["state"]["monitor"]["sourceName"], SOURCE_NAME);
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

    harness.stop_monitor().await.error_for_status().unwrap();
    let stopped = wait_for_snapshot(&mut ws, "monitor stopped snapshot", |snapshot| {
        snapshot["state"]["monitor"]["enabled"] == false && snapshot["state"]["match"].is_null()
    })
    .await;
    assert!(stopped["state"]["recordingState"].is_null());
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
    frame: &support::test_obs::Frame,
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
