use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::support::harness::{API, Harness, SOURCE_NAME, event_ws_url, next_app_snapshot};

const RELEASE_URL: &str = "https://github.com/acheronfail/the_golden_eye/releases/tag/v999.0.0";

/// A provisional update must not announce success until the loader commits it.
/// The settings use a release tag, as the real release server does.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn reload_sends_update_applied_notice_to_new_connections() {
    let harness = Harness::start(Duration::ZERO).await;

    let status: Value =
        harness.client.get(format!("{API}/api/v1/settings/status")).send().await.unwrap().json().await.unwrap();
    let config_path = std::path::PathBuf::from(status["configPath"].as_str().unwrap());

    // ge_runtime_stop drops its own Tokio runtime, which Tokio rejects from an
    // async worker -- same hazard as harness.rs's Drop impl.
    std::thread::spawn(|| ge_runtime::ge_runtime_stop()).join().unwrap();

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(
        &config_path,
        serde_json::to_vec(&json!({
            "lastKnownUpdateVersion": format!("v{}", env!("GE_PLUGIN_VERSION")),
            "lastKnownUpdateReleaseUrl": RELEASE_URL
        }))
        .unwrap(),
    )
    .unwrap();

    ge_runtime::ge_runtime_set_load_context(true, true);
    assert!(ge_runtime::ge_runtime_start(), "server failed to restart");
    ge_runtime::ge_sources_changed();

    let mut ws = harness.connect_event_stream().await;
    let snapshot = next_app_snapshot(&mut ws, "reloaded source snapshot").await;
    assert_eq!(snapshot["state"]["sources"], json!([{"name":SOURCE_NAME,"id":"test_input"}]));
    assert!(harness.obs.calls().source_names > 0, "reload should refresh sources without waiting for FINISHED_LOADING");

    assert_no_update_notice(&mut ws).await;
    let status: Value =
        harness.client.get(format!("{API}/api/v1/updates/status")).send().await.unwrap().json().await.unwrap();
    assert_eq!(status["phase"], "applying");
    ge_runtime::ge_runtime_commit_update();
    let status: Value =
        harness.client.get(format!("{API}/api/v1/updates/status")).send().await.unwrap().json().await.unwrap();
    assert_eq!(status["phase"], "idle");
    let live = wait_for_update_applied_event(&mut ws).await;
    assert_eq!(live["version"], env!("GE_PLUGIN_VERSION"));
    assert_no_update_notice(&mut ws).await;
    let (mut late_ws, _) = connect_async(event_ws_url()).await.unwrap();
    let value = wait_for_update_applied_event(&mut late_ws).await;
    assert_eq!(value["version"], env!("GE_PLUGIN_VERSION"));
    assert_eq!(value["releaseUrl"], RELEASE_URL);

    drop(harness);
}

/// A cold start (the harness's normal path, and every other integration
/// test) must never send this notice.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn cold_start_does_not_send_update_applied_notice() {
    let harness = Harness::start(Duration::ZERO).await;

    let (mut ws, _) = connect_async(event_ws_url()).await.unwrap();
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        if let Ok(Some(Ok(Message::Text(text)))) = tokio::time::timeout(Duration::from_millis(100), ws.next()).await {
            let value: Value = serde_json::from_str(&text).unwrap();
            assert_ne!(value["type"], "updateApplied", "cold start must not send an updateApplied notice");
        }
    }

    drop(harness);
}

async fn wait_for_update_applied_event(
    ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);

    loop {
        match tokio::time::timeout(Duration::from_millis(200), ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let value: Value = serde_json::from_str(&text).unwrap();
                if value["type"] == "updateApplied" {
                    return value;
                }
            }
            Ok(Some(Ok(Message::Binary(bytes)))) => {
                let value: Value = serde_json::from_slice(&bytes).unwrap();
                if value["type"] == "updateApplied" {
                    return value;
                }
            }
            Ok(Some(Ok(Message::Close(frame)))) => {
                panic!("app event stream closed while waiting for updateApplied: {frame:?}");
            }
            Ok(Some(Ok(_))) | Err(_) => {}
            Ok(Some(Err(err))) => panic!("app event stream failed while waiting for updateApplied: {err}"),
            Ok(None) => panic!("app event stream ended while waiting for updateApplied"),
        }

        if Instant::now() >= deadline {
            panic!("timed out waiting for updateApplied event");
        }
    }
}

async fn assert_no_update_notice(
    ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
) {
    let deadline = Instant::now() + Duration::from_millis(300);
    while Instant::now() < deadline {
        if let Ok(Some(Ok(Message::Text(text)))) = tokio::time::timeout(Duration::from_millis(50), ws.next()).await {
            let value: Value = serde_json::from_str(&text).unwrap();
            assert_ne!(value["type"], "updateApplied");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with just test-integration"]
async fn rollback_is_ready_without_an_update_notice() {
    let harness = Harness::start(Duration::ZERO).await;
    std::thread::spawn(|| ge_runtime::ge_runtime_stop()).join().unwrap();
    ge_runtime::ge_runtime_set_load_context(true, false);
    assert!(ge_runtime::ge_runtime_start());
    ge_runtime::ge_sources_changed();
    let mut ws = harness.connect_event_stream().await;
    let snapshot = next_app_snapshot(&mut ws, "rollback source snapshot").await;
    assert_eq!(snapshot["state"]["sources"], json!([{"name": SOURCE_NAME, "id": "test_input"}]));
    assert_no_update_notice(&mut ws).await;
}
