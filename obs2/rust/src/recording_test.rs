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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/clips/sample_clip.mov")
}

fn test_snapshot_store() -> SharedStateStore {
    SharedStateStore::new(AppSnapshot {
        monitor: MonitorSnapshot {
            enabled: true,
            source_name: Some("N64 Capture".to_owned()),
            cv_language: Some("en".to_owned()),
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
        "N64 Capture".to_owned(),
        "en".to_owned(),
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
    recording.pending.as_ref().and_then(|p| p.stats.as_ref()).and_then(|m| m.times)
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
    let mut m = match_with_time();
    m.screen = screen;
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

#[test]
fn padding_defaults_to_five_and_adds_the_internal_buffer_at_both_ends() {
    let default = RecordingOptions::default();
    assert_eq!(default.pre_run_padding_secs, DEFAULT_PRE_RUN_PADDING_SECS);
    assert_eq!(default.post_run_padding_secs, DEFAULT_POST_RUN_PADDING_SECS);
    assert_eq!(default.recent_run_limit, DEFAULT_RECENT_RUN_LIMIT);
    assert_eq!(default.pre_run_padding_secs(), DEFAULT_PRE_RUN_PADDING_SECS + MATCH_PADDING_BUFFER_SECS);
    assert_eq!(default.post_run_padding_secs(), DEFAULT_POST_RUN_PADDING_SECS + MATCH_PADDING_BUFFER_SECS);

    // A configured value of zero still carries the internal safety buffer, so a
    // one-frame timing window can't drop the briefing or stats overlay.
    let zero =
        RecordingOptions { pre_run_padding_secs: 0.0, post_run_padding_secs: 0.0, ..RecordingOptions::default() };
    assert_eq!(zero.pre_run_padding_secs(), MATCH_PADDING_BUFFER_SECS);
    assert_eq!(zero.post_run_padding_secs(), MATCH_PADDING_BUFFER_SECS);

    let negative =
        RecordingOptions { pre_run_padding_secs: -2.0, post_run_padding_secs: -2.0, ..RecordingOptions::default() };
    assert_eq!(negative.pre_run_padding_secs(), MATCH_PADDING_BUFFER_SECS);
    assert_eq!(negative.post_run_padding_secs(), MATCH_PADDING_BUFFER_SECS);
}

#[test]
fn start_then_level_screen_cancels_active_session_without_save() {
    let (mut recording, mut events) = test_recording(RecordingOptions::default());
    let start = Instant::now();

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(3), &match_for_screen(Screen::Unknown));
    recording.on_frame(start + Duration::from_secs(10), &match_for_screen(Screen::Levels));

    assert_eq!(recording.clip_start, None);
    assert_eq!(recording.status, None);
    assert!(recording.report.is_none());
    assert!(recording.pending.is_none());
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::Cancelled));
    assert_no_app_event(&mut events);
}

#[test]
fn failed_report_then_stats_schedules_failed_save() {
    let (mut recording, mut events) = test_recording(RecordingOptions::default());
    let start = Instant::now();
    let failed_at = start + Duration::from_secs(8);
    let stats_at = start + Duration::from_secs(12);

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(5), &match_for_screen(Screen::Unknown));
    recording.on_frame(failed_at, &match_for_screen(Screen::Failed));

    assert_eq!(recording.status, Some(RunStatus::Failed));
    assert_eq!(recording.report.as_ref().map(|m| m.screen), Some(Screen::Failed));
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::Failed));

    recording.on_frame(stats_at, &match_with_time());

    let pending = pending_save_event(&mut events);
    assert!(pending.failed);
    assert_eq!(pending.status, "failed");
    assert_eq!(pending.time_secs, Some(123));
    assert!((pending.estimated_duration_secs - 23.0).abs() < f64::EPSILON);
    assert_eq!(recording.clip_start, None);
    assert_eq!(recording.status, None);
    assert!(recording.report.is_none());
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::SavePending));
    let replay_saves = recording.replay_saves.current();
    assert_eq!(replay_saves.len(), 1);
    assert_eq!(replay_saves[0].save_id, pending.save_id);
    assert_eq!(replay_saves[0].stage, ReplaySaveStage::Scheduled);

    let job = recording.take_pending_job(stats_at + Duration::from_secs(5)).expect("save job");
    assert_eq!(recording.replay_saves.current()[0].stage, ReplaySaveStage::WaitingForReplaySave);
    assert_eq!(job.status, RunStatus::Failed);
    assert_eq!(job.stats.as_ref().map(|m| m.screen), Some(Screen::Stats));
    assert!((job.start_before_save_secs - 22.5).abs() < f64::EPSILON);
    assert_eq!(job.trim_tail_secs, 0.0);
}

#[test]
fn complete_report_then_stats_schedules_completed_save() {
    let (mut recording, mut events) = test_recording(RecordingOptions::default());
    let start = Instant::now();
    let complete_at = start + Duration::from_secs(20);
    let stats_at = start + Duration::from_secs(22);

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(5), &match_for_screen(Screen::Unknown));
    recording.on_frame(complete_at, &match_for_screen(Screen::Complete));

    assert_eq!(recording.status, Some(RunStatus::Complete));
    assert_eq!(recording.report.as_ref().map(|m| m.screen), Some(Screen::Complete));
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::Complete));

    recording.on_frame(stats_at, &match_with_time());

    let pending = pending_save_event(&mut events);
    assert!(!pending.failed);
    assert_eq!(pending.status, "complete");
    assert_eq!(pending.time_secs, Some(123));
    assert!((pending.estimated_duration_secs - 33.0).abs() < f64::EPSILON);
    assert_eq!(recording.clip_start, None);
    assert_eq!(recording.status, None);
    assert!(recording.report.is_none());
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::SavePending));

    let job = recording.take_pending_job(stats_at + Duration::from_secs(5)).expect("save job");
    assert_eq!(job.status, RunStatus::Complete);
    assert_eq!(job.stats.as_ref().map(|m| m.screen), Some(Screen::Stats));
    assert!(matches!(
        events.try_recv().expect("catalog row event"),
        AppEvent::RunCatalogChanged { run_id: Some(run_id), save_id: Some(save_id) }
            if run_id == job.metadata.run_id && save_id == job.save_id
    ));
}

#[test]
fn catalog_failure_still_saves_a_tagged_clip_and_recovers_the_run_row() {
    let dir = TestDir::new("catalog-failure-save");
    let catalog = Arc::new(crate::db::run_catalog::RunCatalog::open(dir.join("runs.sqlite")).unwrap());
    let replay = sample_clip();
    let old = catalog
        .create_finalized_run(
            UNIX_EPOCH + Duration::from_secs(1),
            clip_metadata(
                RunStatus::Failed,
                UNIX_EPOCH + Duration::from_secs(1),
                Some(&match_with_time()),
                "N64 Capture",
                "en",
            ),
        )
        .unwrap();
    let old_path = dir.join("clips/old.mov");
    fs::create_dir_all(old_path.parent().unwrap()).unwrap();
    let duration = ffmpeg::duration_secs(&replay).unwrap();
    ffmpeg::trim_with_metadata(&replay, &old_path, 1.0, duration - 1.0, Some(&old.metadata)).unwrap();
    catalog
        .record_saved_clip(RunCatalogSave {
            path: old_path.clone(),
            duration_secs: Some(duration - 2.0),
            metadata: old.metadata,
        })
        .unwrap();
    catalog.set_fail_create_finalized(true);
    let (event_tx, mut events) = tokio::sync::broadcast::channel(8);
    let options = RecordingOptions {
        completed_output_path: dir.join("clips").to_string_lossy().into_owned(),
        recent_run_limit: 1,
        ..RecordingOptions::default()
    };
    let snapshot = test_snapshot_store();
    let mut recording = RecordingState::new(
        event_tx,
        RecordingStateStore::new(snapshot.clone()),
        ReplaySaveStateStore::new(snapshot),
        options.clone(),
        "N64 Capture".to_owned(),
        "en".to_owned(),
        catalog.clone(),
    );
    let now = Instant::now();
    recording.status = Some(RunStatus::Complete);
    assert!(recording.schedule_save(now, now - Duration::from_secs(10), Some(match_with_time())));
    let pending = pending_save_event(&mut events);
    let job = recording.take_pending_job(now + Duration::from_secs(5)).expect("catalog failure must not drop save");
    assert_eq!(job.save_id, pending.save_id);
    assert!(!job.metadata.run_id.is_empty());
    assert_no_app_event(&mut events);
    assert!(old_path.exists(), "lowering the retention limit alone must not delete clips");

    catalog.set_fail_create_finalized(false);
    let saved = trim_clip(TrimClipRequest {
        save_id: job.save_id,
        replay_path: replay.to_str().unwrap(),
        start_before_save_secs: 2.0,
        trim_tail_secs: 0.0,
        status: job.status,
        completed_at: job.completed_at,
        stats: job.stats,
        metadata: job.metadata.clone(),
        options: &options,
        run_catalog: &catalog,
    })
    .expect("save tagged clip");

    assert!(Path::new(&saved.path).is_file());
    assert!(!old_path.exists(), "retention cleanup should run after the new clip is attached");
    let recovered = catalog.get_run(&job.metadata.run_id).unwrap().expect("saved clip should recreate catalog row");
    assert!(recovered.clip.is_some());
}

#[test]
fn single_stats_frame_trusts_its_reading() {
    let (mut recording, mut events) = test_recording_saving_short_failed_runs();
    let start = Instant::now();

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(5), &match_for_screen(Screen::Kia));
    recording.on_frame(start + Duration::from_secs(10), &stats_match(14));

    let pending = pending_save_event(&mut events);
    assert_eq!(pending.time_secs, Some(14));
    assert_eq!(pending_stats_time(&recording), Some(14));
    recording.pending = None;
}

#[test]
fn first_stats_frame_misread_is_corrected_by_later_frames() {
    let (mut recording, mut events) = test_recording_saving_short_failed_runs();
    let start = Instant::now();
    let stats_at = start + Duration::from_secs(10);

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(5), &match_for_screen(Screen::Kia));

    // First stats frame misreads the time; the save is scheduled off it.
    recording.on_frame(stats_at, &stats_match(374));
    let pending = pending_save_event(&mut events);
    assert_eq!(pending.time_secs, Some(374));
    assert_eq!(pending_stats_time(&recording), Some(374));

    // Subsequent stable frames outvote the misread, correcting the pending time.
    recording.on_frame(stats_at + Duration::from_millis(16), &stats_match(14));
    recording.on_frame(stats_at + Duration::from_millis(32), &stats_match(14));
    assert_eq!(pending_stats_time(&recording), Some(14));

    recording.pending = None;
}

#[test]
fn two_stats_frames_trust_the_second_reading() {
    let (mut recording, _events) = test_recording_saving_short_failed_runs();
    let start = Instant::now();
    let stats_at = start + Duration::from_secs(10);

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(5), &match_for_screen(Screen::Kia));
    recording.on_frame(stats_at, &stats_match(374));
    recording.on_frame(stats_at + Duration::from_millis(16), &stats_match(14));

    assert_eq!(pending_stats_time(&recording), Some(14));
    recording.pending = None;
}

#[test]
fn best_time_flicker_is_outvoted_independently_of_the_run_time() {
    // The dimmer best-time row flickers between the true 28 and a 20 misread
    // while the run time and target stay steady. Each field votes on its own,
    // so best-time settles on the majority 28 even though the final frame read
    // 20 -- the exact live capture-card symptom this guards against.
    let (mut recording, _events) = test_recording_saving_short_failed_runs();
    let start = Instant::now();
    let mut at = start + Duration::from_secs(10);
    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(5), &match_for_screen(Screen::Kia));

    for best in [Some(28), Some(28), Some(20), Some(28), Some(28), Some(20)] {
        recording.on_frame(at, &stats_match_full(28, Some(300), best));
        at += Duration::from_millis(16);
    }

    let times = pending_stats_times(&recording).expect("stats times");
    assert_eq!(times.time, 28);
    assert_eq!(times.target_time, Some(300));
    assert_eq!(times.best_time, Some(28), "majority best-time wins, not the last flicker frame");
    recording.pending = None;
}

#[test]
fn run_time_flicker_does_not_disturb_the_voted_best_time() {
    // The reverse independence: a flickering run time must not drag the stable
    // best/target with it when the newest frame becomes the naming source.
    let (mut recording, _events) = test_recording_saving_short_failed_runs();
    let start = Instant::now();
    let mut at = start + Duration::from_secs(10);
    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(5), &match_for_screen(Screen::Kia));

    for time in [123, 123, 999, 123, 999, 123] {
        recording.on_frame(at, &stats_match_full(time, Some(100), Some(130)));
        at += Duration::from_millis(16);
    }

    let times = pending_stats_times(&recording).expect("stats times");
    assert_eq!(times.time, 123);
    assert_eq!(times.target_time, Some(100));
    assert_eq!(times.best_time, Some(130));
    recording.pending = None;
}

#[test]
fn persistent_first_frame_misread_is_outvoted_by_the_stable_reading() {
    // The misread spans several frames (as it can live, where the transitional
    // overlay frame is matched more than once), yet the stable reading fills the
    // rest of the window and wins -- there is no fixed sampling cap to defeat.
    let (mut recording, mut events) = test_recording_saving_short_failed_runs();
    let start = Instant::now();
    let mut at = start + Duration::from_secs(10);

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(5), &match_for_screen(Screen::Kia));

    recording.on_frame(at, &stats_match(374));
    let _ = pending_save_event(&mut events);
    for _ in 0..2 {
        at += Duration::from_millis(16);
        recording.on_frame(at, &stats_match(374));
    }
    // Still on the (persisted) misread after three frames.
    assert_eq!(pending_stats_time(&recording), Some(374));

    for _ in 0..5 {
        at += Duration::from_millis(16);
        recording.on_frame(at, &stats_match(14));
    }
    assert_eq!(pending_stats_time(&recording), Some(14));
    recording.pending = None;
}

#[test]
fn pending_event_is_reissued_when_the_voted_time_changes() {
    let (mut recording, mut events) = test_recording_saving_short_failed_runs();
    let start = Instant::now();
    let stats_at = start + Duration::from_secs(10);

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(5), &match_for_screen(Screen::Kia));

    recording.on_frame(stats_at, &stats_match(374));
    let first = pending_save_event(&mut events);
    assert_eq!(first.time_secs, Some(374));

    // A newer, differing reading replaces the provisional row under the same id.
    recording.on_frame(stats_at + Duration::from_millis(16), &stats_match(14));
    let updated = pending_save_event(&mut events);
    assert_eq!(updated.save_id, first.save_id);
    assert_eq!(updated.time_secs, Some(14));

    // A repeat of the settled reading doesn't spam another event.
    recording.on_frame(stats_at + Duration::from_millis(32), &stats_match(14));
    assert_no_app_event(&mut events);
    recording.pending = None;
}

#[test]
fn leaving_the_stats_screen_locks_the_voted_time() {
    let (mut recording, mut events) = test_recording_saving_short_failed_runs();
    let start = Instant::now();
    let stats_at = start + Duration::from_secs(10);

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(5), &match_for_screen(Screen::Kia));
    recording.on_frame(stats_at, &stats_match(14));
    let _ = pending_save_event(&mut events);

    // Once the screen leaves stats, a later stats reading (e.g. a new run's
    // screen within the padding window) must not change this save's time.
    recording.on_frame(stats_at + Duration::from_millis(16), &match_for_screen(Screen::Unknown));
    recording.on_frame(stats_at + Duration::from_millis(32), &stats_match(999));

    assert_eq!(pending_stats_time(&recording), Some(14));
    recording.pending = None;
}

#[test]
fn poll_pending_waits_for_the_padding_window_before_firing() {
    let (mut recording, mut events) = test_recording_saving_short_failed_runs();
    let start = Instant::now();
    let stats_at = start + Duration::from_secs(10);

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(5), &match_for_screen(Screen::Kia));
    recording.on_frame(stats_at, &stats_match(14));
    let _ = pending_save_event(&mut events);

    // The fire time is the run finish plus the post-run padding, independent of
    // when frames arrive; polling before it elapses is a no-op.
    let fire_at = recording.pending_fire_at().expect("pending fire time");
    assert_eq!(fire_at, stats_at + recording.options.save_delay());
    recording.poll_pending(fire_at - Duration::from_millis(1));
    assert!(recording.pending.is_some());
    recording.pending = None;
}

#[test]
fn complete_report_then_level_screen_saves_as_stats_skipped() {
    let (mut recording, mut events) = test_recording(RecordingOptions::default());
    let start = Instant::now();
    let complete_at = start + Duration::from_secs(20);
    let levels_at = start + Duration::from_secs(24);

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(complete_at, &match_for_screen(Screen::Complete));
    recording.on_frame(levels_at, &match_for_screen(Screen::Levels));

    let pending = pending_save_event(&mut events);
    assert!(!pending.failed);
    assert_eq!(pending.status, "complete");
    assert_eq!(pending.time_secs, None);
    assert_eq!(pending.stats.as_ref().map(|m| m.screen), Some(Screen::Complete));
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::StatsSkipped));

    let job = recording.take_pending_job(levels_at + Duration::from_secs(5)).expect("save job");
    assert_eq!(job.status, RunStatus::Complete);
    assert_eq!(job.stats.as_ref().map(|m| m.screen), Some(Screen::Complete));
}

#[test]
fn failed_report_then_level_screen_schedules_save_without_stats_skipped() {
    let (mut recording, mut events) = test_recording(RecordingOptions::default());
    let start = Instant::now();
    let failed_at = start + Duration::from_secs(20);
    let levels_at = start + Duration::from_secs(24);

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(failed_at, &match_for_screen(Screen::Failed));
    recording.on_frame(levels_at, &match_for_screen(Screen::Levels));

    let pending = pending_save_event(&mut events);
    assert!(pending.failed);
    assert_eq!(pending.status, "failed");
    assert_eq!(pending.time_secs, None);
    assert_eq!(pending.stats.as_ref().map(|m| m.screen), Some(Screen::Failed));
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::SavePending));

    let job = recording.take_pending_job(levels_at + Duration::from_secs(5)).expect("save job");
    assert_eq!(job.status, RunStatus::Failed);
    assert_eq!(job.stats.as_ref().map(|m| m.screen), Some(Screen::Failed));
}

#[test]
fn late_save_completion_does_not_clear_a_second_runs_matching_phase() {
    // Two completed runs that both skip stats land on the same phase value
    // (`StatsSkipped`). Run 1's save completing late must not clear run 2's
    // still-in-flight phase -- only run 2's own save completing should.
    let (mut recording, mut events) = test_recording(RecordingOptions::default());
    let start = Instant::now();

    // Run 1: completes, skips stats (backs out via the grid).
    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(10), &match_for_screen(Screen::Complete));
    recording.on_frame(start + Duration::from_secs(12), &match_for_screen(Screen::Levels));
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::StatsSkipped));
    let _ = pending_save_event(&mut events);

    // Take run 1's job directly (as the save timer would), without going
    // through the real save thread, so nothing flushes automatically below.
    let job1 = recording.take_pending_job(start + Duration::from_secs(17)).expect("run 1 save job");
    let generation1 = job1.phase_generation.expect("run 1 emitted a phase generation");

    // Run 2 starts (quick restart) and also completes, skipping stats too --
    // landing on the same `StatsSkipped` value, with a newer generation.
    recording.on_frame(start + Duration::from_secs(13), &match_for_screen(Screen::Start));
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::Started));
    recording.on_frame(start + Duration::from_secs(20), &match_for_screen(Screen::Complete));
    recording.on_frame(start + Duration::from_secs(22), &match_for_screen(Screen::Levels));
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::StatsSkipped));
    let _ = pending_save_event(&mut events);
    let replay_saves = recording.replay_saves.current();
    assert_eq!(replay_saves.len(), 2);
    assert!(replay_saves.iter().any(|save| save.stage == ReplaySaveStage::WaitingForReplaySave));
    assert!(replay_saves.iter().any(|save| save.stage == ReplaySaveStage::Scheduled));

    // Run 1's save completes late: clearing by its own (stale) generation
    // must leave run 2's `StatsSkipped` phase untouched.
    recording.recording_state.clear_if_generation(generation1);
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::StatsSkipped));

    // Run 2's own save completing does clear it.
    let job2 = recording.take_pending_job(start + Duration::from_secs(27)).expect("run 2 save job");
    let generation2 = job2.phase_generation.expect("run 2 emitted a phase generation");
    recording.recording_state.clear_if_generation(generation2);
    assert_eq!(recording.recording_state.current(), None);
}

#[test]
fn complete_report_after_failure_clears_failure_and_saves_completed_stats() {
    let (mut recording, mut events) = test_recording(RecordingOptions::default());
    let start = Instant::now();
    let stats_at = start + Duration::from_secs(15);

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(8), &match_for_screen(Screen::Failed));
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::Failed));

    recording.on_frame(start + Duration::from_secs(10), &match_for_screen(Screen::Complete));
    assert_eq!(recording.status, Some(RunStatus::Complete));
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::Complete));

    recording.on_frame(stats_at, &match_with_time());

    let pending = pending_save_event(&mut events);
    assert!(!pending.failed);
    assert_eq!(pending.status, "complete");
    assert_eq!(pending.time_secs, Some(123));

    let job = recording.take_pending_job(stats_at + Duration::from_secs(5)).expect("save job");
    assert_eq!(job.status, RunStatus::Complete);
    assert_eq!(job.stats.as_ref().map(|m| m.screen), Some(Screen::Stats));
}

#[test]
fn terminal_screens_without_active_session_are_ignored() {
    let (mut recording, mut events) = test_recording(RecordingOptions::default());
    let now = Instant::now();

    for screen in [Screen::Failed, Screen::Abort, Screen::Kia, Screen::Complete, Screen::Stats, Screen::Levels] {
        let m = if screen == Screen::Stats { match_with_time() } else { match_for_screen(screen) };
        recording.on_frame(now, &m);
        assert_eq!(recording.clip_start, None);
        assert_eq!(recording.status, None);
        assert!(recording.report.is_none());
        assert!(recording.pending.is_none());
        assert_eq!(recording.recording_state.current(), None);
        assert_no_app_event(&mut events);
    }
}

#[test]
fn duplicate_start_frames_do_not_reset_the_session_anchor() {
    let (mut recording, mut events) = test_recording(RecordingOptions::default());
    let start = Instant::now();
    let duplicate_start = start + Duration::from_secs(10);
    let stats_at = start + Duration::from_secs(20);

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(duplicate_start, &match_for_screen(Screen::Start));
    assert_eq!(recording.clip_start, Some(start));

    recording.on_frame(stats_at, &match_with_time());

    let pending = pending_save_event(&mut events);
    assert_eq!(pending.status, "complete");
    assert!((pending.estimated_duration_secs - 31.0).abs() < f64::EPSILON);

    let job = recording.take_pending_job(stats_at + Duration::from_secs(5)).expect("save job");
    assert_eq!(job.status, RunStatus::Complete);
    assert!((job.start_before_save_secs - 30.5).abs() < f64::EPSILON);
}

#[test]
fn failure_screen_variants_emit_distinct_statuses_and_save_statuses() {
    for (screen, recording_status, run_status, pending_status) in [
        (Screen::Failed, RecordingStatus::Failed, RunStatus::Failed, "failed"),
        (Screen::Abort, RecordingStatus::Aborted, RunStatus::Abort, "abort"),
        (Screen::Kia, RecordingStatus::Kia, RunStatus::Kia, "kia"),
    ] {
        let (mut recording, mut events) = test_recording(RecordingOptions::default());
        let start = Instant::now();
        let stats_at = start + Duration::from_secs(12);

        recording.on_frame(start, &match_for_screen(Screen::Start));
        recording.on_frame(start + Duration::from_secs(10), &match_for_screen(screen));

        assert_eq!(recording.status, Some(run_status));
        assert_eq!(recording.report.as_ref().map(|m| m.screen), Some(screen));
        assert_eq!(recording.recording_state.current(), Some(recording_status));

        recording.on_frame(stats_at, &match_with_time());

        let pending = pending_save_event(&mut events);
        assert!(pending.failed);
        assert_eq!(pending.status, pending_status);

        let job = recording.take_pending_job(stats_at + Duration::from_secs(5)).expect("save job");
        assert_eq!(job.status, run_status);
        // The emitted `SavePending` phase is tracked by generation for cleanup.
        assert!(job.phase_generation.is_some());
    }
}

#[test]
fn output_dir_prefers_configured_path_then_replay_parent() {
    let dir = TestDir::new("output-dir");
    let input = dir.join("replay.mov");
    let completed = dir.join("completed");
    let mut options = RecordingOptions {
        completed_output_path: completed.to_string_lossy().into_owned(),
        ..RecordingOptions::default()
    };

    assert_eq!(output_dir(&input, &options), completed);

    options.completed_output_path.clear();
    assert_eq!(output_dir(&input, &options), dir.path());
}

#[test]
fn ensure_output_directory_creates_nested_missing_directory() {
    let dir = TestDir::new("ensure-output");
    let output = dir.join("completed/deeply/nested");

    assert!(!output.exists());
    ensure_output_directory(&output).unwrap();

    assert!(output.is_dir());
}

#[test]
fn ensure_output_directory_rejects_existing_file() {
    let dir = TestDir::new("ensure-output-file");
    let output = dir.join("completed");
    write_file(&output);

    let err = ensure_output_directory(&output).unwrap_err();

    assert!(
        err.to_string().contains("creating output directory")
            || err.to_string().contains("exists but is not a directory"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn shutdown_before_pending_save_fires_waits_and_preserves_save_job() {
    let options =
        RecordingOptions { pre_run_padding_secs: 1.0, post_run_padding_secs: 5.0, ..RecordingOptions::default() };
    let (mut recording, mut events) = test_recording(options);
    let start = Instant::now();
    let stats_at = start + Duration::from_secs(10);

    assert!(recording.schedule_save(stats_at, start, Some(match_with_time())));

    let pending = events.try_recv().expect("pending save event");
    let AppEvent::RecordingSavePending(pending) = pending else {
        panic!("expected pending save event");
    };
    assert_eq!(pending.save_id, 1);
    assert_eq!(pending.save_in_secs, 5.5);
    assert_eq!(pending.level, "Surface 2");
    assert_eq!(pending.time_secs, Some(123));

    let slept = RefCell::new(None);
    let saved_job = RefCell::new(None);
    recording.flush_pending_on_shutdown_with(
        stats_at + Duration::from_secs(2),
        |duration| *slept.borrow_mut() = Some(duration),
        |job| *saved_job.borrow_mut() = Some(job),
    );

    assert_eq!(*slept.borrow(), Some(Duration::from_secs_f64(3.5)));
    let job = saved_job.borrow_mut().take().expect("save job");
    assert_eq!(job.save_id, 1);
    assert_eq!(job.status, RunStatus::Complete);
    assert!(job.completed_at <= SystemTime::now());
    assert_eq!(job.stats.as_ref().and_then(|m| m.times).map(|times| times.time), Some(123));
    assert_eq!(job.options.pre_run_padding_secs, 1.0);
    assert_eq!(job.options.post_run_padding_secs, 5.0);
    assert_eq!(job.metadata.source_name, "N64 Capture");
    assert_eq!(job.metadata.rom_language, "en");
    assert_eq!(job.event_tx.receiver_count(), 1);
    assert_eq!(job.recording_state.current(), None);
    assert!((job.start_before_save_secs - 17.0).abs() < f64::EPSILON);
    assert_eq!(job.trim_tail_secs, 0.0);
    assert!(recording.pending.is_none());
}

#[test]
fn shutdown_after_pending_save_fire_time_flushes_without_waiting() {
    let options =
        RecordingOptions { pre_run_padding_secs: 1.0, post_run_padding_secs: 5.0, ..RecordingOptions::default() };
    let (mut recording, _events) = test_recording(options);
    let start = Instant::now();
    let stats_at = start + Duration::from_secs(10);

    assert!(recording.schedule_save(stats_at, start, Some(match_with_time())));

    let slept = RefCell::new(None);
    let saved_job = RefCell::new(None);
    recording.flush_pending_on_shutdown_with(
        stats_at + Duration::from_secs(7),
        |duration| *slept.borrow_mut() = Some(duration),
        |job| *saved_job.borrow_mut() = Some(job),
    );

    assert_eq!(*slept.borrow(), None);
    let job = saved_job.borrow_mut().take().expect("save job");
    assert_eq!(job.save_id, 1);
    assert!((job.start_before_save_secs - 18.5).abs() < f64::EPSILON);
    assert_eq!(job.trim_tail_secs, 1.5);
    assert!(recording.pending.is_none());
}

#[test]
fn remove_replay_file_after_trim_deletes_replay_and_keeps_saved_clip() {
    let dir = TestDir::new("remove-replay");
    let replay = dir.join("obs replay.mov");
    let saved = dir.join("trimmed clip.mov");
    write_file(&replay);
    write_file(&saved);

    remove_replay_file_after_trim(&replay.to_string_lossy(), &saved.to_string_lossy());

    assert!(!replay.exists());
    assert!(saved.exists());
}

#[test]
fn remove_replay_file_after_trim_skips_when_paths_match() {
    let dir = TestDir::new("remove-replay-same-path");
    let saved = dir.join("clip.mov");
    write_file(&saved);

    remove_replay_file_after_trim(&saved.to_string_lossy(), &saved.to_string_lossy());

    assert!(saved.exists());
}

#[test]
fn new_replay_files_reports_only_matching_files_added_after_the_snapshot() {
    let dir = TestDir::new("new-replay-files");
    let existing = dir.join("existing.mp4");
    write_file(&existing);

    let before = snapshot_replay_files(dir.path());
    let added = dir.join("obs-replay.mp4");
    let other_ext = dir.join("notes.txt");
    write_file(&added);
    write_file(&other_ext);

    let new_files = new_replay_files(dir.path(), &before, &added.to_string_lossy());

    // Only the newly-added file with the saved file's extension counts: the
    // pre-existing file and the unrelated `.txt` are both excluded.
    assert_eq!(new_files, vec![added]);
}

#[test]
fn resolve_saved_replay_trusts_the_single_new_file_over_the_event_path() {
    let event_path = "/replays/user-save.mp4".to_owned();
    let ours = PathBuf::from("/replays/our-save.mp4");

    let resolved = resolve_saved_replay(event_path, vec![ours.clone()]);

    assert_eq!(resolved.path, ours.to_string_lossy());
    assert!(resolved.safe_to_delete);
}

#[test]
fn resolve_saved_replay_keeps_source_when_a_concurrent_save_is_ambiguous() {
    let event_path = "/replays/reported.mp4".to_owned();
    let a = PathBuf::from("/replays/a.mp4");
    let b = PathBuf::from("/replays/b.mp4");

    // Two files appeared, so we can't tell ours from the user's: fall back to
    // OBS's reported path but never delete it.
    let resolved = resolve_saved_replay(event_path.clone(), vec![a, b]);
    assert_eq!(resolved.path, event_path);
    assert!(!resolved.safe_to_delete);

    // No new file at all is treated the same conservative way.
    let resolved = resolve_saved_replay(event_path.clone(), vec![]);
    assert_eq!(resolved.path, event_path);
    assert!(!resolved.safe_to_delete);
}

#[test]
fn unique_output_path_chooses_first_available_numeric_suffix() {
    let dir = TestDir::new("unique-output");
    let base = dir.join("clip.mp4");
    let second = dir.join("clip (2).mp4");
    write_file(&base);
    write_file(&second);

    let third = dir.join("clip (3).mp4");
    assert_eq!(unique_output_path(&base), third);
    assert!(!third.exists());

    let no_ext = dir.join("clip");
    write_file(&no_ext);
    assert_eq!(unique_output_path(&no_ext), dir.join("clip (2)"));
}

#[test]
fn render_clip_template_replaces_all_supported_tokens() {
    let m = match_with_time();
    let completed_at = UNIX_EPOCH + Duration::from_secs(1_700_000_000);

    let rendered = render_clip_template(
        "{obs_replay_name}|{mission}|{part}|{levelNumber}|{level}|{time}|{difficulty}|{status}|{timestamp}|{timestamp_local}",
        "obs replay",
        RunStatus::Complete,
        completed_at,
        Some(&m),
    );

    assert_eq!(
        rendered,
        format!(
            "obs replay|05|1|8|Surface 2|02:03|00 Agent|complete|2023-11-14T22:13:20Z|{}",
            format_iso_local(completed_at),
        ),
    );
}

#[test]
fn render_clip_template_uses_empty_fields_without_stats() {
    let rendered = render_clip_template(
        "{level}|{mission}|{part}|{levelNumber}|{time}|{difficulty}|{status}|{obs_replay_name}",
        "replay",
        RunStatus::Failed,
        UNIX_EPOCH,
        None,
    );

    assert_eq!(rendered, "unknown||||||failed|replay");
}

#[test]
fn render_clip_template_omits_time_when_report_has_no_stats_row() {
    let m = match_without_time();

    let rendered = render_clip_template(
        "{mission}-{part}-{levelNumber}-{level}-{time}-{difficulty}-{status}",
        "replay",
        RunStatus::Abort,
        UNIX_EPOCH,
        Some(&m),
    );

    assert_eq!(rendered, "01-2-2-Facility--Secret Agent-abort");
}

#[test]
fn render_clip_template_marks_unreadable_header_parts() {
    let m = match_with_unreadable_header();

    let rendered = render_clip_template(
        "{mission}|{part}|{levelNumber}|{level}|{time}|{difficulty}|{status}",
        "replay",
        RunStatus::Kia,
        UNIX_EPOCH,
        Some(&m),
    );

    assert_eq!(rendered, "??|?||unknown|00:00||kia");
}

#[test]
fn render_clip_template_leaves_unknown_tokens_and_unsanitized_text() {
    let m = match_with_time();

    let rendered = render_clip_template(
        "{obs_replay_name}/{not_a_token}/{level}:{status}",
        "OBS/Replay:01",
        RunStatus::Complete,
        UNIX_EPOCH,
        Some(&m),
    );

    assert_eq!(rendered, "OBS/Replay:01/{not_a_token}/Surface 2:complete");
}

#[test]
fn clip_template_renders_and_sanitizes_relative_paths() {
    let m = match_with_time();

    let rendered = render_clip_template(
        "{obs_replay_name}-{mission}-{part}-{levelNumber}-{level}-{time}-{difficulty}-{status}-{timestamp}",
        "obs replay",
        RunStatus::Abort,
        UNIX_EPOCH,
        Some(&m),
    );
    assert_eq!(rendered, "obs replay-05-1-8-Surface 2-02:03-00 Agent-abort-1970-01-01T00:00:00Z");

    let path = clip_relative_path(
        "OBS/Replay:01",
        RunStatus::Kia,
        UNIX_EPOCH,
        Some(&m),
        &format!(
            "{{level}}{}{{difficulty}}{}{{time}}?{{status}}",
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR
        ),
    );
    let name = path.file_name().and_then(|name| name.to_str()).unwrap();
    for forbidden in ['/', '\\', ':', '*', '?', '"', '<', '>', '|'] {
        assert!(!name.contains(forbidden), "{name:?} still contains {forbidden:?}");
    }
    assert_eq!(path.parent().unwrap(), Path::new("Surface 2").join("00 Agent"));
    assert!(name.contains("02-03"));
    assert!(name.ends_with("-kia"));

    assert_eq!(
        clip_relative_path("replay", RunStatus::Complete, UNIX_EPOCH, None, "..."),
        PathBuf::from(format!("unknown -  -  - {}", sanitize_path_component(&format_iso_local(UNIX_EPOCH)))),
    );
}

#[test]
fn clip_template_rejects_traversal_and_wrong_platform_separator() {
    let m = match_with_time();

    assert_eq!(
        clip_relative_path("replay", RunStatus::Complete, UNIX_EPOCH, Some(&m), "../{level}"),
        default_flat_clip_path_for_surface_2(UNIX_EPOCH),
    );
    assert_eq!(
        clip_relative_path(
            "replay",
            RunStatus::Complete,
            UNIX_EPOCH,
            Some(&m),
            &format!("{{level}}{}..{}{{time}}", std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR),
        ),
        default_flat_clip_path_for_surface_2(UNIX_EPOCH),
    );
    assert_eq!(
        clip_relative_path(
            "replay",
            RunStatus::Complete,
            UNIX_EPOCH,
            Some(&m),
            if std::path::MAIN_SEPARATOR == '/' { "{level}\\{time}" } else { "{level}/{time}" },
        ),
        default_flat_clip_path_for_surface_2(UNIX_EPOCH),
    );
}
