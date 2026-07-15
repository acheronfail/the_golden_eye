use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::routing::get;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame::Video;
use ffmpeg_next::software::scaling::context::Context as ScalingContext;
use ffmpeg_next::software::scaling::flag::Flags as ScalingFlags;
use ffmpeg_next::{codec, format, media};
use futures_util::StreamExt;
use opencv::prelude::*;
use opencv::{imgcodecs, imgproc};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use super::test_obs::{Config, Frame, TestObs};

pub const API: &str = "http://127.0.0.1:31337";
pub const SOURCE_NAME: &str = "GoldenEye Capture";

pub struct Harness {
    pub root: PathBuf,
    pub temp: PathBuf,
    pub replay_dir: PathBuf,
    pub client: reqwest::Client,
    pub obs: TestObs,
}

impl Harness {
    pub async fn start(replay_stop_delay: Duration) -> Self {
        let root = repo_root();
        let temp = test_dir();
        let replay_dir = temp.join("replays");
        let fixture = root.join("test/clips/replay-buffer-60s.mp4");
        assert!(
            fixture.is_file(),
            "replay fixture is missing at {}; regenerate it with test/clips/generate_replay_fixture.sh",
            fixture.display()
        );

        // The integration recipe runs ignored tests serially, so changing config
        // env before the backend creates threads is safe and keeps SettingsStore
        // away from developer/CI runner settings. Windows code reads APPDATA/
        // USERPROFILE instead of HOME/XDG_CONFIG_HOME (default_settings_path in
        // settings.rs, home_dir in http/routes/{folders,runs}.rs) -- without
        // overriding those too, every test on Windows shares the real,
        // persistent %APPDATA%\The Golden Eye\settings.json, so state like
        // `lastUpdateCheckTime` leaks across tests (and processes).
        unsafe {
            std::env::set_var("HOME", &temp);
            std::env::set_var("XDG_CONFIG_HOME", temp.join(".config"));
            std::env::set_var("APPDATA", &temp);
            std::env::set_var("USERPROFILE", &temp);
        }

        // `ge_rust_start` unconditionally kicks off a background update check
        // (see `updates::check_for_updates_on_startup`); without an override it
        // hits the real GitHub API on every single test run. Point it at a
        // local "nothing new" mock instead, unless the test already set its
        // own (update_apply.rs/update_checking.rs configure one that actually
        // reports an update, which must win).
        if std::env::var_os("GE_UPDATE_CHECK_URL").is_none() {
            let mock_url = start_local_update_mock().await;
            unsafe { std::env::set_var("GE_UPDATE_CHECK_URL", mock_url) };
        }

        // Lives under the per-test temp dir (not the repo's real build
        // directory) so tests that derive an install/staging directory from
        // this path (e.g. update_apply.rs) get one that's isolated and
        // writable, and cleaned up with everything else in `temp`.
        let core_path = temp.join("golden_core.test");

        let obs = TestObs::install(Config {
            data_path: root.join("obs2"),
            binary_path: core_path.clone(),
            replay_output_directory: replay_dir.clone(),
            replay_fixture: fixture,
            fps: 59.94,
            replay_enabled: true,
            replay_available: true,
            replay_active: false,
            replay_max_seconds: 60,
            replay_stop_delay,
            replay_save_delay: Duration::ZERO,
            sources: vec![(SOURCE_NAME.into(), "test_input".into())],
        });

        // Normally set by core.c's ge_core_load (see lib.rs::core_path); the
        // harness calls ge_rust_start() directly, bypassing that C layer
        // entirely, so it has to set this itself -- and must do so on every
        // call (not just once), since this same process runs multiple tests
        // that each want their own isolated path.
        let core_path_c = std::ffi::CString::new(core_path.to_string_lossy().into_owned()).unwrap();
        unsafe { ge_rust::ge_rust_set_core_path(core_path_c.as_ptr()) };

        assert!(ge_rust::ge_rust_start(), "server failed to start");
        let client = reqwest::Client::new();
        wait_for_server(&client).await;

        Self { root, temp, replay_dir, client, obs }
    }

    pub fn frame(&self, relative: &str) -> Frame {
        load_bgra(&self.root.join(relative))
    }

    pub async fn put_settings(&self, settings: Value) {
        self.client
            .put(format!("{API}/api/v1/settings"))
            .json(&settings)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }

    pub async fn start_monitor(&self) -> reqwest::Response {
        self.client
            .post(format!("{API}/api/v1/monitor/start"))
            .json(&json!({"sourceName": SOURCE_NAME}))
            .send()
            .await
            .unwrap()
    }

    pub async fn stop_monitor(&self) -> reqwest::Response {
        self.client.post(format!("{API}/api/v1/monitor/stop")).send().await.unwrap()
    }

    pub async fn wait_for_status(&self, predicate: impl Fn(&Value) -> bool) -> Value {
        wait_for_monitor_snapshot(predicate).await
    }

    pub async fn render_until_state(&self, frame: &Frame, expected: &str) {
        let (mut ws, _) = connect_async("ws://127.0.0.1:31337/api/v1/monitor/ws").await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut last_status = Value::Null;
        loop {
            self.obs.render(frame.clone());
            match tokio::time::timeout(Duration::from_millis(120), ws.next()).await {
                Ok(Some(Ok(message))) => {
                    if let Some(snapshot) = snapshot_from_message(message) {
                        last_status = snapshot["state"]["monitor"].clone();
                        if snapshot["state"]["recordingState"] == expected {
                            return;
                        }
                    }
                }
                Ok(Some(Err(err))) => {
                    panic!("monitor websocket failed while waiting for recording state {expected}: {err}")
                }
                Ok(None) => panic!("monitor websocket ended while waiting for recording state {expected}"),
                Err(_) => {}
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for recording state {expected}; last monitor: {last_status}"
            );
        }
    }

    pub async fn wait_for_replay_inactive(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.obs.replay_active() {
            assert!(Instant::now() < deadline, "timed out waiting for replay buffer to stop");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    pub async fn connect_monitor_ws(&self) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
        connect_async("ws://127.0.0.1:31337/api/v1/monitor/ws").await.unwrap().0
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        // ge_rust_stop drops its own Tokio runtime, which Tokio rejects from an
        // async worker. Run it on a plain thread and wait for full teardown.
        // Any in-flight update check is torn down along with it, so it's safe
        // to clear the env var below without racing a still-running fetch.
        std::thread::spawn(|| ge_rust::ge_rust_stop()).join().unwrap();

        // Undoes whichever of {our default mock, a test's own override} was in
        // effect, so a later test in the same process doesn't inherit a URL
        // pointing at a mock server that no longer exists.
        unsafe { std::env::remove_var("GE_UPDATE_CHECK_URL") };

        if std::env::var_os("GE_KEEP_INTEGRATION_OUTPUTS").is_some() {
            eprintln!("kept integration outputs at {}", self.temp.display());
        } else if let Err(error) = fs::remove_dir_all(&self.temp) {
            eprintln!("failed to remove integration directory {}: {error}", self.temp.display());
        }
    }
}

pub fn recording_settings(completed: &Path, failed: &Path) -> Value {
    json!({
        "completedOutputPath": completed,
        "failedOutputPath": failed,
        "saveFailedRuns": true,
        "minimumFailedRunLengthSecs": 0,
        "failedRunLimit": 0,
        "clipFilenameTemplate": "integration-{status}-{level}",
        "preRunPaddingSecs": 0,
        "postRunPaddingSecs": 0,
        "discordNotificationsEnabled": false
    })
}

pub async fn wait_for_clip(dir: &Path) -> PathBuf {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(path) = output_clip(dir)
            && try_probe_duration(&path).is_some_and(|duration| duration > 0.0)
        {
            return path;
        }
        assert!(Instant::now() < deadline, "timed out waiting for a trimmed clip in {}", dir.display());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub fn probe_duration(path: &Path) -> f64 {
    try_probe_duration(path).unwrap_or_else(|| panic!("ffprobe could not read {}", path.display()))
}

fn try_probe_duration(path: &Path) -> Option<f64> {
    ffmpeg_next::init().ok()?;
    let input = format::input(path).ok()?;
    Some(input.duration() as f64 / 1_000_000.0)
}

/// Decode the six little-endian barcode boxes burned into the replay fixture.
pub fn visual_second(path: &Path, offset: f64) -> u8 {
    let frame = decode_rgb_frame_at(path, offset)
        .unwrap_or_else(|| panic!("linked FFmpeg could not decode {} at {offset:.3}s", path.display()));
    assert_eq!((frame.width, frame.height), (640, 360));
    (0..6).fold(0, |value, bit| {
        let x = 104 + bit * 72 + 28;
        let pixel = frame.data[(325 * frame.width as usize + x) * 3];
        value | (u8::from(pixel > 200) << bit)
    })
}

/// Decode every frame of `path` into BGRA `Frame`s in presentation order,
/// matching the pixel layout OBS hands the matcher. Used to replay a real
/// capture clip through the live monitor loop.
pub fn decode_bgra_frames(path: &Path) -> Vec<Frame> {
    ffmpeg_next::init().expect("init ffmpeg");
    let mut input = format::input(path).unwrap_or_else(|err| panic!("open {}: {err}", path.display()));
    let stream = input.streams().best(media::Type::Video).expect("video stream");
    let stream_index = stream.index();
    let context = codec::context::Context::from_parameters(stream.parameters()).expect("codec context");
    let mut decoder = context.decoder().video().expect("video decoder");
    let mut scaler = ScalingContext::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::BGRA,
        decoder.width(),
        decoder.height(),
        ScalingFlags::BILINEAR,
    )
    .expect("scaler");

    let mut frames = Vec::new();
    for (packet_stream, packet) in input.packets() {
        if packet_stream.index() != stream_index {
            continue;
        }
        decoder.send_packet(&packet).expect("send packet");
        drain_bgra_frames(&mut decoder, &mut scaler, &mut frames);
    }
    decoder.send_eof().expect("send eof");
    drain_bgra_frames(&mut decoder, &mut scaler, &mut frames);
    frames
}

fn drain_bgra_frames(decoder: &mut ffmpeg_next::decoder::Video, scaler: &mut ScalingContext, frames: &mut Vec<Frame>) {
    let mut decoded = Video::empty();
    while decoder.receive_frame(&mut decoded).is_ok() {
        let mut bgra = Video::empty();
        scaler.run(&decoded, &mut bgra).expect("scale to bgra");
        let width = bgra.width();
        let height = bgra.height();
        let stride = bgra.stride(0);
        let source = bgra.data(0);
        let mut data = vec![0u8; width as usize * height as usize * 4];
        for y in 0..height as usize {
            let source_row = &source[y * stride..][..width as usize * 4];
            let output_row = &mut data[y * width as usize * 4..][..width as usize * 4];
            output_row.copy_from_slice(source_row);
        }
        frames.push(Frame { width, height, bgra: data });
    }
}

struct RgbFrame {
    data: Vec<u8>,
    width: u32,
    height: u32,
}

fn decode_rgb_frame_at(path: &Path, offset: f64) -> Option<RgbFrame> {
    ffmpeg_next::init().ok()?;
    let mut input = format::input(path).ok()?;
    let stream = input.streams().best(media::Type::Video)?;
    let stream_index = stream.index();
    let time_base = f64::from(stream.time_base());
    let context = codec::context::Context::from_parameters(stream.parameters()).ok()?;
    let mut decoder = context.decoder().video().ok()?;
    let mut scaler = ScalingContext::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGB24,
        decoder.width(),
        decoder.height(),
        ScalingFlags::POINT,
    )
    .ok()?;
    let mut first_timestamp = None;

    for (packet_stream, packet) in input.packets() {
        if packet_stream.index() != stream_index {
            continue;
        }
        decoder.send_packet(&packet).ok()?;
        if let Some(frame) = receive_target_frame(&mut decoder, &mut scaler, time_base, offset, &mut first_timestamp) {
            return Some(frame);
        }
    }

    decoder.send_eof().ok()?;
    receive_target_frame(&mut decoder, &mut scaler, time_base, offset, &mut first_timestamp)
}

fn receive_target_frame(
    decoder: &mut ffmpeg_next::decoder::Video,
    scaler: &mut ScalingContext,
    time_base: f64,
    target_offset: f64,
    first_timestamp: &mut Option<f64>,
) -> Option<RgbFrame> {
    let mut decoded = Video::empty();
    while decoder.receive_frame(&mut decoded).is_ok() {
        let timestamp = decoded.timestamp().unwrap_or(0) as f64 * time_base;
        let start = *first_timestamp.get_or_insert(timestamp);
        if timestamp - start + 0.000_001 < target_offset {
            continue;
        }

        let mut rgb = Video::empty();
        scaler.run(&decoded, &mut rgb).ok()?;
        let width = rgb.width();
        let height = rgb.height();
        let stride = rgb.stride(0);
        let source = rgb.data(0);
        let mut data = vec![0; width as usize * height as usize * 3];
        for y in 0..height as usize {
            let source_row = &source[y * stride..][..width as usize * 3];
            let output_row = &mut data[y * width as usize * 3..][..width as usize * 3];
            output_row.copy_from_slice(source_row);
        }
        return Some(RgbFrame { data, width, height });
    }
    None
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn test_dir() -> PathBuf {
    let serial = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("golden-eye-integration-{}-{serial}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn load_bgra(path: &Path) -> Frame {
    let bgr = imgcodecs::imread(path.to_str().unwrap(), imgcodecs::IMREAD_COLOR).unwrap();
    let mut bgra = Mat::default();
    imgproc::cvt_color(&bgr, &mut bgra, imgproc::COLOR_BGR2BGRA, 0, opencv::core::AlgorithmHint::ALGO_HINT_DEFAULT)
        .unwrap();
    Frame { width: bgra.cols() as u32, height: bgra.rows() as u32, bgra: bgra.data_bytes().unwrap().to_vec() }
}

fn output_clip(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("mp4"))
}

/// Serves a GitHub-releases-shaped "nothing new" response (`tag_name` equal to
/// this build's own version, so `updates::update_from_release` finds it not
/// newer) on an ephemeral local port, for tests that don't care about the
/// update flow. Never explicitly shut down -- it's spawned on the calling
/// `#[tokio::test]`'s own runtime, which aborts it along with every other task
/// when that runtime drops at the end of the test.
async fn start_local_update_mock() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new().route("/latest", get(no_update_available));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}/latest")
}

async fn no_update_available() -> axum::Json<Value> {
    axum::Json(json!({
        "tag_name": env!("GE_PLUGIN_VERSION"),
        "html_url": "https://github.com/acheronfail/the_golden_eye/releases",
        "assets": []
    }))
}

async fn wait_for_server(client: &reqwest::Client) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if client.get(format!("{API}/api/v1/settings/status")).send().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("test plugin HTTP server did not start");
}

async fn wait_for_monitor_snapshot(predicate: impl Fn(&Value) -> bool) -> Value {
    let (mut ws, _) = connect_async("ws://127.0.0.1:31337/api/v1/monitor/ws").await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut last_status = Value::Null;
    loop {
        match tokio::time::timeout(Duration::from_millis(200), ws.next()).await {
            Ok(Some(Ok(message))) => {
                if let Some(snapshot) = snapshot_from_message(message) {
                    let status = snapshot["state"]["monitor"].clone();
                    last_status = status.clone();
                    if predicate(&status) {
                        return status;
                    }
                }
            }
            Ok(Some(Err(err))) => panic!("monitor websocket failed while waiting for status: {err}"),
            Ok(None) => panic!("monitor websocket ended while waiting for status"),
            Err(_) => {}
        }
        assert!(Instant::now() < deadline, "timed out waiting for monitor status; last: {last_status}");
    }
}

pub async fn next_monitor_snapshot(ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>, label: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match tokio::time::timeout(Duration::from_millis(200), ws.next()).await {
            Ok(Some(Ok(message))) => {
                if let Some(snapshot) = snapshot_from_message(message) {
                    return snapshot;
                }
            }
            Ok(Some(Err(err))) => panic!("monitor websocket failed while waiting for {label}: {err}"),
            Ok(None) => panic!("monitor websocket ended while waiting for {label}"),
            Err(_) => {}
        }
        assert!(Instant::now() < deadline, "timed out waiting for {label}");
    }
}

pub fn snapshot_from_message(message: Message) -> Option<Value> {
    let value: Value = match message {
        Message::Text(text) => serde_json::from_str(&text).unwrap(),
        Message::Binary(bytes) => serde_json::from_slice(&bytes).unwrap(),
        Message::Close(frame) => panic!("monitor websocket closed while waiting for snapshot: {frame:?}"),
        _ => return None,
    };
    (value["type"] == "snapshot").then_some(value)
}
