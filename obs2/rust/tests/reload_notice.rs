mod support;

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde_json::{Value, json};
use support::harness::{API, Harness, SOURCE_NAME, next_app_snapshot};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const RELEASE_URL: &str = "https://github.com/acheronfail/the_golden_eye/releases/tag/v999.0.0";

/// Exercises the same signal a real reload sends: `ge_core_load` calls
/// `ge_rust_set_was_reloaded(true)` before `ge_rust_start()` when the load
/// followed a successful update apply (see `obs2/core/core.c`). The harness
/// bypasses the C shim entirely (it calls `ge_rust_start()` directly), so
/// this simulates the same sequence by hand: stop, mark the next start as a
/// reload, start again -- then confirms a freshly connecting client gets the
/// one-off `updateApplied` notice.
///
/// `lastKnownUpdateVersion`/`lastKnownUpdateReleaseUrl` are backend-owned
/// (like `lastUpdateCheckTime`), so `PUT /api/v1/settings` can never set them
/// -- and a real update check (see `update_checking.rs`/`update_apply.rs`)
/// can only ever persist a version strictly newer than this test binary's own
/// `GE_PLUGIN_VERSION`, so it could never record a "last known update" equal
/// to the version this same process reports once "reloaded". Writing the
/// settings file directly while the server is stopped simulates the real
/// post-reload world instead: the freshly (re)loaded binary's own version
/// equals the update that was staged for it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn reload_sends_update_applied_notice_to_new_connections() {
    let harness = Harness::start(Duration::ZERO).await;

    let status: Value =
        harness.client.get(format!("{API}/api/v1/settings/status")).send().await.unwrap().json().await.unwrap();
    let config_path = std::path::PathBuf::from(status["configPath"].as_str().unwrap());

    // ge_rust_stop drops its own Tokio runtime, which Tokio rejects from an
    // async worker -- same hazard as harness.rs's Drop impl.
    std::thread::spawn(|| ge_rust::ge_rust_stop()).join().unwrap();

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(
        &config_path,
        serde_json::to_vec(&json!({
            "lastKnownUpdateVersion": env!("GE_PLUGIN_VERSION"),
            "lastKnownUpdateReleaseUrl": RELEASE_URL
        }))
        .unwrap(),
    )
    .unwrap();

    ge_rust::ge_rust_set_was_reloaded(true);
    assert!(ge_rust::ge_rust_start(), "server failed to restart");
    ge_rust::ge_sources_changed();

    let mut ws = harness.connect_event_stream().await;
    let snapshot = next_app_snapshot(&mut ws, "reloaded source snapshot").await;
    assert_eq!(snapshot["state"]["sources"], json!([{"name":SOURCE_NAME,"id":"test_input"}]));
    assert!(harness.obs.calls().source_names > 0, "reload should refresh sources without waiting for FINISHED_LOADING");

    let value = wait_for_update_applied_event().await;
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

    let (mut ws, _) = connect_async("ws://127.0.0.1:31337/api/v1/events/ws").await.unwrap();
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        if let Ok(Some(Ok(Message::Text(text)))) = tokio::time::timeout(Duration::from_millis(100), ws.next()).await {
            let value: Value = serde_json::from_str(&text).unwrap();
            assert_ne!(value["type"], "updateApplied", "cold start must not send an updateApplied notice");
        }
    }

    drop(harness);
}

async fn wait_for_update_applied_event() -> Value {
    let (mut ws, _) = connect_async("ws://127.0.0.1:31337/api/v1/events/ws").await.unwrap();
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
