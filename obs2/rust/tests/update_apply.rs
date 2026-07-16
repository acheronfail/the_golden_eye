mod support;

use std::io::Write as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::routing::get;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use support::harness::{API, Harness};
use tokio::sync::oneshot;

const LATEST_VERSION: &str = "v999.0.0";
const LATEST_ASSET_VERSION: &str = "999.0.0";
const RELEASE_URL: &str = "https://github.com/acheronfail/the_golden_eye/releases/tag/v999.0.0";
const CORE_MARKER_CONTENT: &[u8] = b"fake newer core library contents";

// Mirrors the small alias table in update_apply.rs. Serving all of them
// (pointing at the same asset) rather than trying to guess which one this
// test's own OS/arch maps to keeps the test itself platform-independent.
const PLATFORM_ARCH_SUFFIXES: &[&str] =
    &["macos-arm64", "macos-x86_64", "linux-x86_64", "linux-arm64", "windows-x86_64"];

fn asset_name(suffix: &str) -> String {
    format!("the_golden_eye-{LATEST_ASSET_VERSION}-{suffix}.zip")
}

struct MockState {
    base_url: String,
    zip_bytes: Vec<u8>,
    checksums_text: String,
}

fn build_zip(core_leaf: &str, contents: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        writer.start_file(core_leaf, options).unwrap();
        writer.write_all(contents).unwrap();
        // A real release package also bundles data directories; include an
        // empty one to confirm staging doesn't choke when they're present
        // but irrelevant to this test's assertions.
        writer.add_directory("cv_templates", options).unwrap();
        writer.finish().unwrap();
    }
    buf
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

async fn latest_release(State(state): State<Arc<MockState>>) -> axum::Json<Value> {
    let assets: Vec<Value> = PLATFORM_ARCH_SUFFIXES
        .iter()
        .map(|suffix| {
            json!({
                "name": asset_name(suffix),
                "browser_download_url": format!("{}/asset.zip", state.base_url)
            })
        })
        .chain(std::iter::once(json!({
            "name": "checksums.txt",
            "browser_download_url": format!("{}/checksums.txt", state.base_url)
        })))
        .collect();
    axum::Json(json!({ "tag_name": LATEST_VERSION, "html_url": RELEASE_URL, "assets": assets }))
}

async fn asset_zip(State(state): State<Arc<MockState>>) -> Bytes {
    Bytes::from(state.zip_bytes.clone())
}

async fn checksums_txt(State(state): State<Arc<MockState>>) -> String {
    state.checksums_text.clone()
}

/// Binds the listener first (to learn its own address), then builds the
/// route state with that address baked into the asset download URLs it
/// hands out -- avoiding a placeholder-URL rewrite step.
async fn start_mock_github(
    core_leaf: &str,
    correct_checksum: bool,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{addr}");

    let zip_bytes = build_zip(core_leaf, CORE_MARKER_CONTENT);
    let real_hash = sha256_hex(&zip_bytes);
    let hash = if correct_checksum { real_hash } else { "0".repeat(64) };
    let mut checksums_text = String::new();
    for suffix in PLATFORM_ARCH_SUFFIXES {
        checksums_text.push_str(&format!("{hash}  {}\n", asset_name(suffix)));
    }

    let state = Arc::new(MockState { base_url: base_url.clone(), zip_bytes, checksums_text });
    let app = Router::new()
        .route("/latest", get(latest_release))
        .route("/asset.zip", get(asset_zip))
        .route("/checksums.txt", get(checksums_txt))
        .with_state(state);

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    (base_url, shutdown_tx, server)
}

/// Like `MockState`, but its `/latest` reports the *current* running version
/// (i.e. "nothing new") on the first request and only `LATEST_VERSION` from
/// the second request onward -- for proving that the manual "check now"
/// endpoint actually re-checks rather than just replaying whatever the one
/// automatic startup check already found.
struct SequencedMockState {
    base_url: String,
    zip_bytes: Vec<u8>,
    checksums_text: String,
    calls: std::sync::atomic::AtomicUsize,
}

async fn sequenced_latest_release(State(state): State<Arc<SequencedMockState>>) -> axum::Json<Value> {
    let call = state.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let tag_name = if call == 0 { env!("GE_PLUGIN_VERSION").to_owned() } else { LATEST_VERSION.to_owned() };
    let assets: Vec<Value> = PLATFORM_ARCH_SUFFIXES
        .iter()
        .map(|suffix| {
            json!({
                "name": asset_name(suffix),
                "browser_download_url": format!("{}/asset.zip", state.base_url)
            })
        })
        .chain(std::iter::once(json!({
            "name": "checksums.txt",
            "browser_download_url": format!("{}/checksums.txt", state.base_url)
        })))
        .collect();
    axum::Json(json!({ "tag_name": tag_name, "html_url": RELEASE_URL, "assets": assets }))
}

async fn sequenced_asset_zip(State(state): State<Arc<SequencedMockState>>) -> Bytes {
    Bytes::from(state.zip_bytes.clone())
}

async fn sequenced_checksums_txt(State(state): State<Arc<SequencedMockState>>) -> String {
    state.checksums_text.clone()
}

async fn start_sequenced_mock_github(core_leaf: &str) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{addr}");

    let zip_bytes = build_zip(core_leaf, CORE_MARKER_CONTENT);
    let hash = sha256_hex(&zip_bytes);
    let mut checksums_text = String::new();
    for suffix in PLATFORM_ARCH_SUFFIXES {
        checksums_text.push_str(&format!("{hash}  {}\n", asset_name(suffix)));
    }

    let state = Arc::new(SequencedMockState {
        base_url: base_url.clone(),
        zip_bytes,
        checksums_text,
        calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let app = Router::new()
        .route("/latest", get(sequenced_latest_release))
        .route("/asset.zip", get(sequenced_asset_zip))
        .route("/checksums.txt", get(sequenced_checksums_txt))
        .with_state(state);

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    (base_url, shutdown_tx, server)
}

async fn wait_for_staged_core(core_path: &std::path::Path) -> Vec<u8> {
    let staged = core_path.parent().unwrap().join(".ge_update_staged").join(core_path.file_name().unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(bytes) = tokio::fs::read(&staged).await {
            return bytes;
        }
        assert!(Instant::now() < deadline, "timed out waiting for a staged update at {}", staged.display());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// `trigger_apply` (update_apply.rs) deliberately fires `ge_core_trigger_reload`
/// from a detached `std::thread` rather than the request-handling task -- see
/// its doc comment -- so `/api/v1/updates/apply` returning 202 only means the
/// trigger was dispatched, not that it has run yet. Poll instead of asserting
/// the call count immediately after the response.
async fn wait_for_core_trigger_reload(harness: &Harness) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while harness.obs.calls().core_trigger_reload == 0 {
        assert!(Instant::now() < deadline, "timed out waiting for ge_core_trigger_reload to be called");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_for_last_check_time(harness: &Harness) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status: Value =
            harness.client.get(format!("{API}/api/v1/settings/status")).send().await.unwrap().json().await.unwrap();
        if status["settings"]["lastUpdateCheckTime"].as_u64().is_some() {
            return status;
        }
        assert!(Instant::now() < deadline, "timed out waiting for the startup check to record its check time");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn assert_not_staged(core_path: &std::path::Path) {
    let staged = core_path.parent().unwrap().join(".ge_update_staged").join(core_path.file_name().unwrap());
    assert!(!staged.exists(), "expected no staged update to appear at {}", staged.display());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn valid_update_is_downloaded_verified_staged_and_can_be_applied() {
    let core_leaf = "golden_core.test";
    let (base_url, shutdown_tx, server) = start_mock_github(core_leaf, true).await;

    // SAFETY: integration tests run serially through the just recipe; set
    // before the backend starts so the startup update task reads the mock
    // endpoint.
    unsafe { std::env::set_var("GE_UPDATE_CHECK_URL", format!("{base_url}/latest")) };

    let harness = Harness::start(Duration::ZERO).await;
    let core_path = harness.temp.join(core_leaf);

    // Auto-update defaults off and startup checks are disabled in the harness,
    // so the download waits for an explicit request.
    assert_not_staged(&core_path);

    // Explicit "download now": downloads, verifies, and stages, blocking until
    // it's ready to apply.
    let download_response = harness.client.post(format!("{API}/api/v1/updates/download")).send().await.unwrap();
    assert_eq!(download_response.status().as_u16(), 204, "download-now should stage the update");

    let staged_bytes = wait_for_staged_core(&core_path).await;
    assert_eq!(staged_bytes, CORE_MARKER_CONTENT);

    // Nothing is monitoring/recording, and an update is staged: applying now
    // should succeed and reach the shim's (faked) reload trigger.
    let response = harness.client.post(format!("{API}/api/v1/updates/apply")).send().await.unwrap();
    assert_eq!(response.status().as_u16(), 202, "apply-now should succeed when idle and staged");
    wait_for_core_trigger_reload(&harness).await;
    assert_eq!(harness.obs.calls().core_trigger_reload, 1);

    drop(harness);
    // SAFETY: remove after the backend has stopped so later tests use defaults.
    unsafe { std::env::remove_var("GE_UPDATE_CHECK_URL") };
    shutdown_tx.send(()).unwrap();
    server.await.unwrap();
}

/// With auto-update opted in, a check downloads and stages on its own -- no
/// explicit "download now" needed. The complement of the manual-download path
/// the other tests exercise.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn check_now_auto_stages_when_auto_update_enabled() {
    let core_leaf = "golden_core.test";
    let (base_url, shutdown_tx, server) = start_mock_github(core_leaf, true).await;

    unsafe { std::env::set_var("GE_UPDATE_CHECK_URL", format!("{base_url}/latest")) };

    let harness = Harness::start_with_settings(Duration::ZERO, json!({ "autoUpdateEnabled": true })).await;
    let core_path = harness.temp.join(core_leaf);

    let check_response = harness.client.post(format!("{API}/api/v1/updates/check")).send().await.unwrap();
    assert!(check_response.status().is_success(), "check-now request failed: {}", check_response.status());

    let staged_bytes = wait_for_staged_core(&core_path).await;
    assert_eq!(staged_bytes, CORE_MARKER_CONTENT);

    drop(harness);
    unsafe { std::env::remove_var("GE_UPDATE_CHECK_URL") };
    shutdown_tx.send(()).unwrap();
    server.await.unwrap();
}

/// With auto-update off (the default), a check finds the newer release but must
/// NOT download or stage it on its own -- the download waits for an explicit
/// request. The direct complement of `check_now_auto_stages_when_auto_update_enabled`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn check_now_does_not_stage_when_auto_update_disabled() {
    let core_leaf = "golden_core.test";
    // A valid release is available to download -- so anything staged would be a
    // gating bug, not a download failure.
    let (base_url, shutdown_tx, server) = start_mock_github(core_leaf, true).await;

    unsafe { std::env::set_var("GE_UPDATE_CHECK_URL", format!("{base_url}/latest")) };

    let harness = Harness::start(Duration::ZERO).await;
    let core_path = harness.temp.join(core_leaf);

    // Auto-update defaults off and startup checks are disabled in the harness,
    // so this manual check should not stage anything.
    let check_response = harness.client.post(format!("{API}/api/v1/updates/check")).send().await.unwrap();
    assert!(check_response.status().is_success(), "check-now request failed: {}", check_response.status());
    let check_body: Value = check_response.json().await.unwrap();
    assert_eq!(
        check_body["update"]["latestVersion"], LATEST_VERSION,
        "check-now should still report the newer release: {check_body}"
    );

    // Nothing should have been staged...
    assert_not_staged(&core_path);
    let status: Value =
        harness.client.get(format!("{API}/api/v1/updates/status")).send().await.unwrap().json().await.unwrap();
    assert_eq!(status["staged"], false, "status endpoint should report nothing staged while auto-update is off");

    // ...and applying is refused because there's nothing staged to apply.
    let apply_response = harness.client.post(format!("{API}/api/v1/updates/apply")).send().await.unwrap();
    assert_eq!(apply_response.status().as_u16(), 404, "apply-now should refuse when nothing is staged");
    assert_eq!(harness.obs.calls().core_trigger_reload, 0);

    // An explicit download is what actually stages it.
    let download_response = harness.client.post(format!("{API}/api/v1/updates/download")).send().await.unwrap();
    assert_eq!(download_response.status().as_u16(), 204, "download-now should stage the found update");
    let staged_bytes = wait_for_staged_core(&core_path).await;
    assert_eq!(staged_bytes, CORE_MARKER_CONTENT);

    drop(harness);
    unsafe { std::env::remove_var("GE_UPDATE_CHECK_URL") };
    shutdown_tx.send(()).unwrap();
    server.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn checksum_mismatch_is_never_staged() {
    let core_leaf = "golden_core.test";
    let (base_url, shutdown_tx, server) = start_mock_github(core_leaf, false).await;

    unsafe { std::env::set_var("GE_UPDATE_CHECK_URL", format!("{base_url}/latest")) };

    let harness = Harness::start(Duration::ZERO).await;
    let core_path = harness.temp.join(core_leaf);

    // A download that fails checksum verification must surface as an error and
    // leave nothing staged.
    let download_response = harness.client.post(format!("{API}/api/v1/updates/download")).send().await.unwrap();
    assert_eq!(download_response.status().as_u16(), 500, "download-now should fail on a checksum mismatch");
    assert_not_staged(&core_path);

    let response = harness.client.post(format!("{API}/api/v1/updates/apply")).send().await.unwrap();
    assert_eq!(response.status().as_u16(), 404, "apply-now should refuse when nothing is staged");
    assert_eq!(harness.obs.calls().core_trigger_reload, 0);

    drop(harness);
    unsafe { std::env::remove_var("GE_UPDATE_CHECK_URL") };
    shutdown_tx.send(()).unwrap();
    server.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn apply_now_is_refused_while_monitoring() {
    let core_leaf = "golden_core.test";
    let (base_url, shutdown_tx, server) = start_mock_github(core_leaf, true).await;

    unsafe { std::env::set_var("GE_UPDATE_CHECK_URL", format!("{base_url}/latest")) };

    let harness = Harness::start(Duration::ZERO).await;
    let core_path = harness.temp.join(core_leaf);
    let download_response = harness.client.post(format!("{API}/api/v1/updates/download")).send().await.unwrap();
    assert_eq!(download_response.status().as_u16(), 204, "download-now should stage the update");
    wait_for_staged_core(&core_path).await;

    let start_response = harness.start_monitor().await;
    assert!(start_response.status().is_success(), "failed to start monitor: {}", start_response.status());

    let response = harness.client.post(format!("{API}/api/v1/updates/apply")).send().await.unwrap();
    assert_eq!(response.status().as_u16(), 409, "apply-now should refuse while a monitor session is active");
    assert_eq!(harness.obs.calls().core_trigger_reload, 0);

    harness.stop_monitor().await;
    drop(harness);
    unsafe { std::env::remove_var("GE_UPDATE_CHECK_URL") };
    shutdown_tx.send(()).unwrap();
    server.await.unwrap();
}

/// Proves the actual bug report this is fixing: the automatic startup check
/// finding nothing must not block a manual "check now" a moment later from
/// finding a release that appeared afterward -- `check_for_updates_now` has
/// to bypass the interval that gates the *automatic* check, not just exist
/// as an endpoint that happens to also work when nothing was blocking it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn manual_check_now_bypasses_the_interval_that_blocked_the_automatic_check() {
    let core_leaf = "golden_core.test";
    let (base_url, shutdown_tx, server) = start_sequenced_mock_github(core_leaf).await;

    unsafe { std::env::set_var("GE_UPDATE_CHECK_URL", format!("{base_url}/latest")) };

    // The automatic startup check (first /latest call) finds nothing new and
    // records last_update_check_time just now -- a hypothetical *second*
    // automatic check moments later would see the weekly interval as not due.
    // It's a background task spawned from ge_rust_start, so wait for its
    // observable effect instead of assuming Harness::start() means it finished.
    let harness = Harness::start_with_settings(Duration::ZERO, json!({ "updateCheckInterval": "weekly" })).await;
    let core_path = harness.temp.join(core_leaf);

    let status = wait_for_last_check_time(&harness).await;
    assert!(status["settings"]["lastUpdateCheckTime"].as_u64().is_some(), "startup check should have run: {status}");

    assert!(!harness.temp.join(".ge_update_staged").join(core_leaf).exists(), "nothing should be staged yet");

    // Manual check-now: the mock's second call reports LATEST_VERSION.
    let check_response = harness.client.post(format!("{API}/api/v1/updates/check")).send().await.unwrap();
    assert!(check_response.status().is_success(), "check-now request failed: {}", check_response.status());
    let check_body: Value = check_response.json().await.unwrap();
    assert_eq!(
        check_body["update"]["latestVersion"], "v999.0.0",
        "manual check should have found the newer release: {check_body}"
    );

    // Auto-update is off, so finding the release doesn't download it -- that
    // waits for an explicit "download now."
    assert_not_staged(&core_path);
    let download_response = harness.client.post(format!("{API}/api/v1/updates/download")).send().await.unwrap();
    assert_eq!(download_response.status().as_u16(), 204, "download-now should stage the found update");

    wait_for_staged_core(&core_path).await;

    let status_response: Value =
        harness.client.get(format!("{API}/api/v1/updates/status")).send().await.unwrap().json().await.unwrap();
    assert_eq!(status_response["staged"], true, "status endpoint should report the update as staged");

    drop(harness);
    unsafe { std::env::remove_var("GE_UPDATE_CHECK_URL") };
    shutdown_tx.send(()).unwrap();
    server.await.unwrap();
}
