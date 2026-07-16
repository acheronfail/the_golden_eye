mod support;

use std::ffi::CString;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Path, State};
use axum::http::{StatusCode, Uri};
use axum::routing::{patch, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use support::harness::{Harness, recording_settings};
use tokio::sync::oneshot;

#[derive(Clone, Debug)]
struct WebhookCall {
    method: &'static str,
    uri: String,
    body: Value,
}

type WebhookCalls = Arc<Mutex<Vec<WebhookCall>>>;

async fn record_post(State(calls): State<WebhookCalls>, uri: Uri, Json(body): Json<Value>) -> Json<Value> {
    calls.lock().unwrap().push(WebhookCall { method: "POST", uri: uri.to_string(), body });
    Json(json!({"id": "discord-message-1"}))
}

async fn record_patch(
    State(calls): State<WebhookCalls>,
    Path(message_id): Path<String>,
    uri: Uri,
    Json(body): Json<Value>,
) -> StatusCode {
    assert_eq!(message_id, "discord-message-1");
    calls.lock().unwrap().push(WebhookCall { method: "PATCH", uri: uri.to_string(), body });
    StatusCode::NO_CONTENT
}

async fn wait_for_calls(calls: &WebhookCalls, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if calls.lock().unwrap().len() >= count {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {count} webhook calls");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn enabled_stream_notifications_post_once_then_edit_once() {
    let calls: WebhookCalls = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/webhook", post(record_post))
        .route("/webhook/messages/{message_id}", patch(record_patch))
        .with_state(calls.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let webhook_url = format!("http://{}/webhook", listener.local_addr().unwrap());
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let webhook_server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    let _harness = Harness::start_with_settings_from_temp(Duration::ZERO, |temp| {
        let mut settings = recording_settings(&temp.join("completed"), &temp.join("failed"));
        settings["discordNotificationsEnabled"] = true.into();
        settings["discordWebhookUrl"] = webhook_url.into();
        settings["streamingStartedMessageTemplate"] = "START {broadcast_url}".into();
        settings["streamingStoppedMessageTemplate"] = "STOP {broadcast_url}".into();
        settings
    })
    .await;

    let service_settings = CString::new(r#"{"service":"YouTube - RTMPS","broadcast_id":"golden-eye-live"}"#).unwrap();
    // SAFETY: service_settings is a valid NUL-terminated string for this call.
    unsafe { ge_rust::ge_stream_notifier_start(service_settings.as_ptr()) };
    wait_for_calls(&calls, 1).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    ge_rust::ge_stream_notifier_stop();
    wait_for_calls(&calls, 2).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let calls = calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].method, "POST");
    assert_eq!(calls[0].uri, "/webhook?wait=true");
    assert_eq!(calls[0].body["content"], "START https://youtu.be/golden-eye-live");
    assert_eq!(calls[1].method, "PATCH");
    assert_eq!(calls[1].uri, "/webhook/messages/discord-message-1");
    assert_eq!(calls[1].body["content"], "STOP https://youtu.be/golden-eye-live");
    assert_eq!(calls[1].body["flags"], 4);

    shutdown_tx.send(()).unwrap();
    webhook_server.await.unwrap();
}
