use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use opencv::prelude::*;
use opencv::{imgcodecs, imgproc};
use serde_json::{Value, json};

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

        // Each integration test is its own Cargo test binary, with exactly one
        // test, so changing HOME before the backend creates threads is safe and
        // keeps SettingsStore away from the developer's real settings.
        unsafe { std::env::set_var("HOME", &temp) };

        let obs = TestObs::install(Config {
            data_path: root.join("obs2"),
            binary_path: root.join("obs2/build/golden_core.test"),
            replay_output_directory: replay_dir.clone(),
            replay_fixture: fixture,
            fps: 59.94,
            replay_enabled: true,
            replay_available: true,
            replay_active: false,
            replay_max_seconds: 60,
            replay_stop_delay,
            sources: vec![(SOURCE_NAME.into(), "test_input".into())],
        });

        ge_rust::ge_rust_start();
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
        wait_for_json(&self.client, "/api/v1/monitor/status", predicate).await
    }

    pub async fn render_until_state(&self, frame: &Frame, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.obs.render(frame.clone());
            tokio::time::sleep(Duration::from_millis(120)).await;
            let status: Value =
                self.client.get(format!("{API}/api/v1/monitor/status")).send().await.unwrap().json().await.unwrap();
            if status["recordingState"] == expected {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for recording state {expected}; last: {status}");
        }
    }

    pub async fn wait_for_replay_inactive(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.obs.replay_active() {
            assert!(Instant::now() < deadline, "timed out waiting for replay buffer to stop");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        // ge_rust_stop drops its own Tokio runtime, which Tokio rejects from an
        // async worker. Run it on a plain thread and wait for full teardown.
        std::thread::spawn(|| ge_rust::ge_rust_stop()).join().unwrap();

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
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1"])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()?.trim().parse().ok()
}

/// Decode the six little-endian barcode boxes burned into the replay fixture.
pub fn visual_second(path: &Path, offset: f64) -> u8 {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-ss", &format!("{offset:.3}"), "-i"])
        .arg(path)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "gray", "-"])
        .output()
        .expect("ffmpeg must be installed for integration tests");
    assert!(output.status.success(), "ffmpeg failed: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout.len(), 640 * 360);
    (0..6).fold(0, |value, bit| {
        let x = 104 + bit * 72 + 28;
        let pixel = output.stdout[325 * 640 + x];
        value | (u8::from(pixel > 200) << bit)
    })
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

async fn wait_for_server(client: &reqwest::Client) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if client.get(format!("{API}/api/v1/monitor/status")).send().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("test plugin HTTP server did not start");
}

async fn wait_for_json(client: &reqwest::Client, path: &str, predicate: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let value: Value = client.get(format!("{API}{path}")).send().await.unwrap().json().await.unwrap();
        if predicate(&value) {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {path}; last response: {value}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
