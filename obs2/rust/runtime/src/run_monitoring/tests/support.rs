use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{fs, io};

use ge_cv::{LevelMatch, Screen};
use ge_game::Times;

use super::clip_output::sanitize_path_component;
use super::clip_saving::SaveAndTrimJob;
use super::run_detection::RunUpdate;
use super::{RecordingOptions, RecordingSessionContext, RunRecorder};
use crate::app::{AppEvent, AppSnapshot, SharedStateStore};
use crate::run_monitoring::RecordingStateStore;
use crate::run_monitoring::publication::{MonitorSnapshot, RecordingSavePending, ReplaySaveStateStore};
use crate::template_tokens::format_iso_local;

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

pub(super) struct TestDir {
    path: PathBuf,
}

impl TestDir {
    pub(super) fn new(label: &str) -> Self {
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

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub(crate) fn test_run_catalog(label: &str) -> Arc<ge_catalog::run_catalog::RunCatalog> {
    let dir = TestDir::new(label);
    let path = dir.path.join("runs.sqlite");
    std::mem::forget(dir);
    Arc::new(ge_catalog::run_catalog::RunCatalog::open(path).expect("open run catalog"))
}

pub(super) fn write_file(path: &Path) {
    fs::write(path, b"clip").unwrap();
}

pub(super) fn sample_clip() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../test/clips/sample_clip.mov")
}

pub(super) fn test_snapshot_store() -> SharedStateStore {
    SharedStateStore::new(AppSnapshot {
        monitor: MonitorSnapshot {
            enabled: true,
            source_name: Some("N64 Capture".to_owned()),
            cv_language: Some("en".to_owned()),
            wall_clocks: crate::run_monitoring::publication::MonitorWallClockState::default(),
        },
        level_match: None,
        run_catalog_sync: None,
        recording_state: None,
        replay_saves: vec![],
        sources: Vec::new(),
        replay_buffer: crate::obs::ReplayBufferStatus {
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
        update: crate::plugin_updates::UpdateStatus::default(),
    })
}

pub(super) fn test_recording(options: RecordingOptions) -> (RunRecorder, tokio::sync::broadcast::Receiver<AppEvent>) {
    let (event_tx, event_rx) = tokio::sync::broadcast::channel(8);
    let snapshot = test_snapshot_store();
    let recording_state = RecordingStateStore::new(snapshot.clone());
    let recording = RunRecorder::new(
        event_tx,
        recording_state,
        ReplaySaveStateStore::new(snapshot),
        options,
        RecordingSessionContext::new("N64 Capture".to_owned(), "en".to_owned(), None),
        test_run_catalog("recording-state"),
    );
    (recording, event_rx)
}

pub(super) fn test_recording_saving_short_failed_runs() -> (RunRecorder, tokio::sync::broadcast::Receiver<AppEvent>) {
    test_recording(RecordingOptions::default())
}

pub(super) fn match_with_time() -> LevelMatch {
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

pub(super) fn stats_match(time: i32) -> LevelMatch {
    let mut m = match_with_time();
    m.times = Some(Times { time, target_time: None, best_time: None });
    m.raw_times = vec![time];
    m
}

pub(super) fn stats_match_full(time: i32, target_time: Option<i32>, best_time: Option<i32>) -> LevelMatch {
    let mut m = match_with_time();
    m.times = Some(Times { time, target_time, best_time });
    m.raw_times = vec![time];
    m
}

pub(super) fn pending_stats_time(recording: &RunRecorder) -> Option<i32> {
    pending_stats_times(recording).map(|times| times.time)
}

pub(super) fn pending_stats_times(recording: &RunRecorder) -> Option<Times> {
    recording.run_detection.pending.as_ref().and_then(|p| p.stats.as_ref()).and_then(|m| m.times)
}

pub(super) fn match_without_time() -> LevelMatch {
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

pub(super) fn default_flat_clip_path_for_surface_2(completed_at: SystemTime) -> PathBuf {
    PathBuf::from(format!(
        "Surface 2 - 00 Agent - 02-03 - {}",
        sanitize_path_component(&format_iso_local(completed_at))
    ))
}

pub(super) fn match_with_unreadable_header() -> LevelMatch {
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

pub(super) fn match_for_screen(screen: Screen) -> LevelMatch {
    match_for_screen_with_identity(screen, 5, 1, 2)
}

pub(super) fn match_for_screen_with_identity(screen: Screen, mission: i32, part: i32, difficulty: i32) -> LevelMatch {
    let mut m = match_with_time();
    m.screen = screen;
    m.mission = mission;
    m.part = part;
    m.difficulty = difficulty;
    m.times = None;
    m.raw_times.clear();
    m
}

pub(super) fn pending_save_event(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> RecordingSavePending {
    loop {
        match events.try_recv().expect("pending save event") {
            AppEvent::RecordingSavePending(pending) => return pending,
            AppEvent::RunCatalogChanged { .. } => continue,
            _ => panic!("expected pending save event"),
        }
    }
}

pub(super) fn assert_no_app_event(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) {
    assert!(matches!(events.try_recv(), Err(tokio::sync::broadcast::error::TryRecvError::Empty)));
}

impl RunRecorder {
    pub(super) fn schedule_save(&mut self, now: Instant, clip_start: Instant, stats: Option<LevelMatch>) -> bool {
        let mut update = RunUpdate::default();
        self.run_detection.schedule_save(now, SystemTime::now(), clip_start, stats, self.detection_policy, &mut update);
        for pending in update.ready {
            self.flush_ready(pending, now);
        }
        self.sync_pending_event(now, update.pending_changed);
        true
    }

    pub(super) fn take_pending_job(&mut self, now: Instant) -> Option<SaveAndTrimJob> {
        let pending = self.run_detection.pending.take()?;
        Some(self.save_pipeline.job(pending, now, self.detection_policy))
    }

    pub(super) fn flush_pending_on_shutdown_with(
        &mut self,
        now: Instant,
        sleep: impl FnOnce(Duration),
        save: impl FnOnce(SaveAndTrimJob),
    ) {
        let Some(pending) = self.run_detection.pending.take() else {
            return;
        };
        self.save_pipeline.flush_on_shutdown_with(pending, now, self.detection_policy, sleep, save);
    }
}
