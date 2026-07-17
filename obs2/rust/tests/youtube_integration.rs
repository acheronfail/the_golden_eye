#[path = "../src/ffmpeg.rs"]
mod ffmpeg;
mod support;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::{Value, json};
use support::harness::{API, Harness, recording_settings};
use tokio::sync::oneshot;

#[derive(Default)]
struct YoutubeMockState {
    token_calls: Mutex<Vec<Value>>,
    upload_chunks: Mutex<Vec<String>>,
}

async fn token_endpoint(State(state): State<Arc<YoutubeMockState>>, body: Bytes) -> Json<Value> {
    let body = String::from_utf8_lossy(&body);
    state.token_calls.lock().unwrap().push(json!({ "body": body.to_string() }));
    Json(json!({
        "access_token": "access-token",
        "refresh_token": "refresh-token",
        "expires_in": 3600,
        "scope": "openid email profile https://www.googleapis.com/auth/youtube.upload",
        "token_type": "Bearer"
    }))
}

async fn userinfo_endpoint() -> Json<Value> {
    Json(json!({
        "email": "runner@example.com",
        "name": "Runner Account",
        "picture": "https://example.test/avatar.png"
    }))
}

async fn start_upload(State(state): State<Arc<YoutubeMockState>>, headers: HeaderMap) -> Response {
    state.upload_chunks.lock().unwrap().push("start".to_owned());
    let host = headers.get(axum::http::header::HOST).and_then(|value| value.to_str().ok()).unwrap();
    let mut response = StatusCode::OK.into_response();
    response
        .headers_mut()
        .insert(axum::http::header::LOCATION, HeaderValue::from_str(&format!("http://{host}/session")).unwrap());
    response
}

async fn upload_chunk(State(state): State<Arc<YoutubeMockState>>, headers: HeaderMap) -> impl IntoResponse {
    let range = headers
        .get(axum::http::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    state.upload_chunks.lock().unwrap().push(range);
    Json(json!({ "id": "video-123" }))
}

async fn start_youtube_mock() -> (String, Arc<YoutubeMockState>, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let state = Arc::new(YoutubeMockState::default());
    let app = Router::new()
        .route("/token", post(token_endpoint))
        .route("/userinfo", get(userinfo_endpoint))
        .route("/upload", post(start_upload))
        .route("/session", put(upload_chunk))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });
    (format!("http://{addr}"), state, shutdown_tx, handle)
}

fn set_youtube_env(base_url: &str, token_file: &std::path::Path) {
    unsafe {
        std::env::set_var("GE_YOUTUBE_ENABLED", "1");
        std::env::set_var("GE_TEST_YOUTUBE_OAUTH_STATE", "test-state");
        std::env::set_var("GE_TEST_YOUTUBE_TOKEN_FILE", token_file);
        std::env::set_var("GE_TEST_YOUTUBE_CLIENT_ID", "test-client");
        std::env::set_var("GE_TEST_YOUTUBE_CLIENT_SECRET", "test-secret");
        std::env::set_var("GE_TEST_YOUTUBE_TOKEN_URL", format!("{base_url}/token"));
        std::env::set_var("GE_TEST_YOUTUBE_USERINFO_URL", format!("{base_url}/userinfo"));
        std::env::set_var("GE_TEST_YOUTUBE_UPLOAD_URL", format!("{base_url}/upload"));
    }
}

fn clear_youtube_env() {
    unsafe {
        for key in [
            "GE_YOUTUBE_ENABLED",
            "GE_TEST_YOUTUBE_OAUTH_STATE",
            "GE_TEST_YOUTUBE_TOKEN_FILE",
            "GE_TEST_YOUTUBE_CLIENT_ID",
            "GE_TEST_YOUTUBE_CLIENT_SECRET",
            "GE_TEST_YOUTUBE_TOKEN_URL",
            "GE_TEST_YOUTUBE_USERINFO_URL",
            "GE_TEST_YOUTUBE_UPLOAD_URL",
        ] {
            std::env::remove_var(key);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn youtube_oauth_dance_connects_and_stores_account_info() {
    let (base_url, mock, shutdown, server) = start_youtube_mock().await;
    let token_file = std::env::temp_dir().join(format!("ge-youtube-oauth-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&token_file);
    set_youtube_env(&base_url, &token_file);
    let harness = Harness::start(Duration::ZERO).await;

    let connect = harness.client.post(format!("{API}/api/v1/youtube/connect")).send();
    let callback = async {
        wait_for_pending_oauth().await;
        harness
            .client
            .get(format!("{API}/oauth/callback?code=test-code&state=test-state"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    };
    let (connect_response, _) = tokio::join!(connect, callback);
    let status: Value = connect_response.unwrap().error_for_status().unwrap().json().await.unwrap();

    assert_eq!(status["connected"], true);
    assert_eq!(status["account"]["email"], "runner@example.com");
    assert_eq!(status["account"]["name"], "Runner Account");
    assert!(mock.token_calls.lock().unwrap()[0]["body"].as_str().unwrap().contains("grant_type=authorization_code"));

    drop(harness);
    clear_youtube_env();
    shutdown.send(()).unwrap();
    server.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn youtube_upload_posts_video_and_persists_history() {
    let (base_url, mock, shutdown, server) = start_youtube_mock().await;
    let token_file = std::env::temp_dir().join(format!("ge-youtube-upload-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&token_file);
    set_youtube_env(&base_url, &token_file);
    let harness = Harness::start_with_settings_from_temp(Duration::ZERO, |temp| {
        recording_settings(&temp.join("clips"), &temp.join("failed"))
    })
    .await;

    connect_youtube(&harness).await;
    let clip = prepare_clip(&harness).await;
    let upload: Value = harness
        .client
        .post(format!("{API}/api/v1/youtube/upload"))
        .json(&json!({ "path": clip }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(upload["state"], "queued");

    let status = wait_for_uploaded(&harness).await;
    assert_eq!(status["uploads"][0]["state"], "uploaded");
    assert_eq!(status["uploads"][0]["videoUrl"], "https://youtu.be/video-123");
    assert_eq!(status["history"][0]["videoId"], "video-123");
    assert!(mock.upload_chunks.lock().unwrap().iter().any(|entry| entry.starts_with("bytes ")));

    drop(harness);
    clear_youtube_env();
    shutdown.send(()).unwrap();
    server.await.unwrap();
}

async fn connect_youtube(harness: &Harness) {
    let connect = harness.client.post(format!("{API}/api/v1/youtube/connect")).send();
    let callback = async {
        wait_for_pending_oauth().await;
        harness.client.get(format!("{API}/oauth/callback?code=test-code&state=test-state")).send().await.unwrap();
    };
    let (response, _) = tokio::join!(connect, callback);
    response.unwrap().error_for_status().unwrap();
}

async fn wait_for_pending_oauth() {
    tokio::time::sleep(Duration::from_millis(100)).await;
}

async fn prepare_clip(harness: &Harness) -> String {
    let source = harness.root.join("test/clips/replay-buffer-60s.mp4");
    let clip = harness.temp.join("clips").join("youtube-test.mov");
    std::fs::create_dir_all(clip.parent().unwrap()).unwrap();
    ffmpeg::trim_with_metadata(
        &source,
        &clip,
        0.0,
        1.0,
        Some(&ffmpeg::ClipMetadata {
            timestamp: "2026-07-18T00:00:00Z".to_owned(),
            time: Some("00:28".to_owned()),
            time_seconds: Some(28),
            level: "Runway".to_owned(),
            level_number: Some(2),
            difficulty: Some("Agent".to_owned()),
            status: "completed".to_owned(),
            rom_language: "en".to_owned(),
            source_name: "GoldenEye Capture".to_owned(),
            comment: String::new(),
            plugin_version: env!("GE_PLUGIN_VERSION").to_owned(),
        }),
    )
    .unwrap();
    clip.to_string_lossy().into_owned()
}

async fn wait_for_uploaded(harness: &Harness) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status: Value = harness
            .client
            .get(format!("{API}/api/v1/youtube/status"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        if status["uploads"]
            .as_array()
            .is_some_and(|uploads| uploads.iter().any(|upload| upload["state"] == "uploaded"))
        {
            return status;
        }
        assert!(Instant::now() < deadline, "timed out waiting for YouTube upload; status: {status}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
