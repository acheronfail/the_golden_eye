use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{fs, io};

use super::*;
use crate::ge::Times;
use crate::http::{AppSnapshot, MonitorSnapshot, SharedStateStore};
use crate::template_tokens::format_iso_local;

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(label: &str) -> Self {
        loop {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!("ge-recording-{label}-{}-{nanos}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return TestDir { path },
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create test dir {}: {err}", path.display()),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn test_run_catalog(label: &str) -> Arc<crate::db::run_catalog::RunCatalog> {
    let dir = TestDir::new(label);
    let path = dir.path.join("runs.sqlite");
    std::mem::forget(dir);
    Arc::new(crate::db::run_catalog::RunCatalog::open(path).expect("open run catalog"))
}

fn write_file(path: &Path) {
    fs::write(path, b"clip").unwrap();
}

fn sample_clip() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../test/clips/sample_clip.mov")
}

fn test_snapshot_store() -> SharedStateStore {
    SharedStateStore::new(AppSnapshot {
        monitor: MonitorSnapshot {
            enabled: true,
            source_name: Some("N64 Capture".to_owned()),
            cv_language: Some("en".to_owned()),
            wall_clocks: crate::http::MonitorWallClockState::default(),
        },
        level_match: None,
        run_catalog_sync: None,
        recording_state: None,
        replay_saves: vec![],
        sources: Vec::new(),
        replay_buffer: crate::http::ReplayBufferStatus {
            enabled: true,
            available: true,
            active: true,
            max_seconds: Some(1200),
            output_directory: Some("/captures".to_owned()),
            default_completed_output_path: Some("/captures/GoldenEye".to_owned()),
        },
        settings_status: crate::settings::SettingsStatus {
            settings: crate::settings::AppSettings::default(),
            defaults: crate::settings::AppSettings::default(),
            config_path: "/tmp/settings.json".to_owned(),
            plugin_version: "test".to_owned(),
            file_error: None,
        },
        update: crate::updates::UpdateStatus::default(),
    })
}

fn test_recording(options: RecordingOptions) -> (RecordingState, tokio::sync::broadcast::Receiver<AppEvent>) {
    let (event_tx, event_rx) = tokio::sync::broadcast::channel(8);
    let snapshot = test_snapshot_store();
    let recording_state = RecordingStateStore::new(snapshot.clone());
    let recording = RecordingState::new(
        event_tx,
        recording_state,
        ReplaySaveStateStore::new(snapshot),
        options,
        super::RecordingSessionContext::new("N64 Capture".to_owned(), "en".to_owned(), None),
        test_run_catalog("recording-state"),
    );
    (recording, event_rx)
}

fn test_recording_saving_short_failed_runs() -> (RecordingState, tokio::sync::broadcast::Receiver<AppEvent>) {
    test_recording(RecordingOptions::default())
}

fn match_with_time() -> LevelMatch {
    LevelMatch {
        screen: Screen::Stats,
        mission: 5,
        part: 1,
        difficulty: 2,
        detected_lang: None,
        times: Some(Times { time: 123, target_time: Some(100), best_time: Some(130) }),
        raw_times: vec![123, 100, 130],
        match_regions: Vec::new(),
        annotation_sets: Vec::new(),
        runtime_ms: 0.0,
    }
}

fn stats_match(time: i32) -> LevelMatch {
    let mut m = match_with_time();
    m.times = Some(Times { time, target_time: None, best_time: None });
    m.raw_times = vec![time];
    m
}

fn stats_match_full(time: i32, target_time: Option<i32>, best_time: Option<i32>) -> LevelMatch {
    let mut m = match_with_time();
    m.times = Some(Times { time, target_time, best_time });
    m.raw_times = vec![time];
    m
}

fn pending_stats_time(recording: &RecordingState) -> Option<i32> {
    pending_stats_times(recording).map(|times| times.time)
}

fn pending_stats_times(recording: &RecordingState) -> Option<Times> {
    recording.tracker.pending.as_ref().and_then(|p| p.stats.as_ref()).and_then(|m| m.times)
}

fn match_without_time() -> LevelMatch {
    LevelMatch {
        screen: Screen::Complete,
        mission: 1,
        part: 2,
        difficulty: 1,
        detected_lang: None,
        times: None,
        raw_times: Vec::new(),
        match_regions: Vec::new(),
        annotation_sets: Vec::new(),
        runtime_ms: 0.0,
    }
}

fn default_flat_clip_path_for_surface_2(completed_at: SystemTime) -> PathBuf {
    PathBuf::from(format!(
        "Surface 2 - 00 Agent - 02-03 - {}",
        sanitize_path_component(&format_iso_local(completed_at))
    ))
}

fn match_with_unreadable_header() -> LevelMatch {
    LevelMatch {
        screen: Screen::Stats,
        mission: -1,
        part: -1,
        difficulty: 99,
        detected_lang: None,
        times: Some(Times { time: -5, target_time: None, best_time: None }),
        raw_times: vec![-5],
        match_regions: Vec::new(),
        annotation_sets: Vec::new(),
        runtime_ms: 0.0,
    }
}

fn match_for_screen(screen: Screen) -> LevelMatch {
    match_for_screen_with_identity(screen, 5, 1, 2)
}

fn match_for_screen_with_identity(screen: Screen, mission: i32, part: i32, difficulty: i32) -> LevelMatch {
    let mut m = match_with_time();
    m.screen = screen;
    m.mission = mission;
    m.part = part;
    m.difficulty = difficulty;
    m.times = None;
    m.raw_times.clear();
    m
}

fn pending_save_event(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> RecordingSavePending {
    loop {
        match events.try_recv().expect("pending save event") {
            AppEvent::RecordingSavePending(pending) => return pending,
            AppEvent::RunCatalogChanged { .. } => continue,
            _ => panic!("expected pending save event"),
        }
    }
}

fn assert_no_app_event(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) {
    assert!(matches!(events.try_recv(), Err(tokio::sync::broadcast::error::TryRecvError::Empty)));
}

