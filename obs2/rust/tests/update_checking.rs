mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use futures_util::StreamExt;
use serde_json::{Value, json};
use support::harness::{API, Harness};
use tokio::sync::oneshot;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const LATEST_VERSION: &str = "v999.0.0";
const RELEASE_URL: &str = "https://github.com/acheronfail/the_golden_eye/releases/tag/v999.0.0";

async fn latest_release(State(calls): State<Arc<AtomicUsize>>) -> axum::Json<Value> {
    calls.fetch_add(1, Ordering::SeqCst);
    axum::Json(json!({
        "tag_name": LATEST_VERSION,
        "html_url": RELEASE_URL
    }))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn startup_update_check_persists_check_time_and_replays_update_event() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = Router::new().route("/latest", get(latest_release)).with_state(calls.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let update_check_url = format!("http://{}/latest", listener.local_addr().unwrap());
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let release_server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    // SAFETY: integration tests run serially through the just recipe; set before
    // the backend starts so the startup update task reads the mock endpoint.
    unsafe { std::env::set_var("GE_UPDATE_CHECK_URL", &update_check_url) };
    let harness = Harness::start(Duration::ZERO).await;

    let update = wait_for_update_available_event(&harness, &calls).await;
    assert_eq!(update["currentVersion"], env!("GE_PLUGIN_VERSION"));
    assert_eq!(update["latestVersion"], LATEST_VERSION);
    assert_eq!(update["releaseUrl"], RELEASE_URL);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let status: Value = harness
        .client
        .get(format!("{API}/api/v1/settings/status"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(status["settings"]["lastUpdateCheckTime"].as_u64().is_some());
    assert_eq!(status["settings"]["updateCheckInterval"], "weekly");

    drop(harness);
    // SAFETY: remove after the backend has stopped so later tests use defaults.
    unsafe { std::env::remove_var("GE_UPDATE_CHECK_URL") };
    shutdown_tx.send(()).unwrap();
    release_server.await.unwrap();
}

async fn wait_for_update_available_event(harness: &Harness, calls: &Arc<AtomicUsize>) -> Value {
    let (mut ws, _) = connect_async("ws://127.0.0.1:31337/api/v1/monitor/ws").await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);

    loop {
        match tokio::time::timeout(Duration::from_millis(200), ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let value: Value = serde_json::from_str(&text).unwrap();
                if value["type"] == "updateAvailable" {
                    return value;
                }
            }
            Ok(Some(Ok(Message::Binary(bytes)))) => {
                let value: Value = serde_json::from_slice(&bytes).unwrap();
                if value["type"] == "updateAvailable" {
                    return value;
                }
            }
            Ok(Some(Ok(Message::Close(frame)))) => {
                panic!("monitor websocket closed while waiting for updateAvailable: {frame:?}");
            }
            Ok(Some(Ok(_))) | Err(_) => {}
            Ok(Some(Err(err))) => panic!("monitor websocket failed while waiting for updateAvailable: {err}"),
            Ok(None) => panic!("monitor websocket ended while waiting for updateAvailable"),
        }

        if Instant::now() >= deadline {
            let status: Value =
                harness.client.get(format!("{API}/api/v1/settings/status")).send().await.unwrap().json().await.unwrap();
            panic!(
                "timed out waiting for updateAvailable event; release calls: {}; settings: {status}",
                calls.load(Ordering::SeqCst)
            );
        }
    }
}
