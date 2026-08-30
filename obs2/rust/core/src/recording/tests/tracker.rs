#[test]
fn padding_defaults_to_five_and_adds_the_internal_buffer_at_both_ends() {
    let default = RecordingOptions::default();
    assert_eq!(default.pre_run_padding_secs, DEFAULT_PRE_RUN_PADDING_SECS);
    assert_eq!(default.post_run_padding_secs, DEFAULT_POST_RUN_PADDING_SECS);
    assert_eq!(default.recent_run_limit, DEFAULT_RECENT_RUN_LIMIT);
    let policy = default.tracker_policy();
    assert_eq!(policy.pre_run_padding_secs, DEFAULT_PRE_RUN_PADDING_SECS + MATCH_PADDING_BUFFER_SECS);
    assert_eq!(policy.post_run_padding_secs, DEFAULT_POST_RUN_PADDING_SECS + MATCH_PADDING_BUFFER_SECS);

    // A configured value of zero still carries the internal safety buffer, so a
    // one-frame timing window can't drop the briefing or stats overlay.
    let zero =
        RecordingOptions { pre_run_padding_secs: 0.0, post_run_padding_secs: 0.0, ..RecordingOptions::default() };
    assert_eq!(zero.tracker_policy().pre_run_padding_secs, MATCH_PADDING_BUFFER_SECS);
    assert_eq!(zero.tracker_policy().post_run_padding_secs, MATCH_PADDING_BUFFER_SECS);

    let negative =
        RecordingOptions { pre_run_padding_secs: -2.0, post_run_padding_secs: -2.0, ..RecordingOptions::default() };
    assert_eq!(negative.tracker_policy().pre_run_padding_secs, MATCH_PADDING_BUFFER_SECS);
    assert_eq!(negative.tracker_policy().post_run_padding_secs, MATCH_PADDING_BUFFER_SECS);
}

#[test]
fn start_then_level_screen_cancels_active_session_without_save() {
    let (mut recording, mut events) = test_recording(RecordingOptions::default());
    let start = Instant::now();

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(3), &match_for_screen(Screen::Unknown));
    recording.on_frame(start + Duration::from_secs(10), &match_for_screen(Screen::Levels));

    assert_eq!(recording.tracker.clip_start, None);
    assert_eq!(recording.tracker.status, None);
    assert!(recording.tracker.report.is_none());
    assert!(recording.tracker.pending.is_none());
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

    assert_eq!(recording.tracker.status, Some(RunStatus::Failed));
    assert_eq!(recording.tracker.report.as_ref().map(|m| m.screen), Some(Screen::Failed));
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::Failed));

    recording.on_frame(stats_at, &match_with_time());

    let pending = pending_save_event(&mut events);
    assert!(pending.failed);
    assert_eq!(pending.status, "failed");
    assert_eq!(pending.time_secs, Some(123));
    assert!((pending.estimated_duration_secs - 23.0).abs() < f64::EPSILON);
    assert_eq!(recording.tracker.clip_start, None);
    assert_eq!(recording.tracker.status, None);
    assert!(recording.tracker.report.is_none());
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

    assert_eq!(recording.tracker.status, Some(RunStatus::Complete));
    assert_eq!(recording.tracker.report.as_ref().map(|m| m.screen), Some(Screen::Complete));
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::Complete));

    recording.on_frame(stats_at, &match_with_time());

    let pending = pending_save_event(&mut events);
    assert!(!pending.failed);
    assert_eq!(pending.status, "complete");
    assert_eq!(pending.time_secs, Some(123));
    assert!((pending.estimated_duration_secs - 33.0).abs() < f64::EPSILON);
    assert_eq!(recording.tracker.clip_start, None);
    assert_eq!(recording.tracker.status, None);
    assert!(recording.tracker.report.is_none());
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
fn save_job_uses_a_clip_limit_changed_during_the_monitor_session() {
    let options = RecordingOptions { recent_run_limit: 5, ..RecordingOptions::default() };
    let (mut recording, mut events) = test_recording(options);
    let live_limit = Arc::new(AtomicUsize::new(20));
    recording.set_recent_run_limit_source(live_limit);
    let start = Instant::now();
    let stats_at = start + Duration::from_secs(10);

    recording.on_frame(start, &match_for_screen(Screen::Start));
    recording.on_frame(start + Duration::from_secs(5), &match_for_screen(Screen::Complete));
    recording.on_frame(stats_at, &match_with_time());
    let _ = pending_save_event(&mut events);

    let job = recording.take_pending_job(stats_at + Duration::from_secs(5)).expect("save job");
    assert_eq!(job.recent_run_limit.load(Ordering::Acquire), 20);
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
    let duration = ge_media::duration_secs(&replay).unwrap();
    ge_media::trim_with_metadata(&replay, &old_path, 1.0, duration - 1.0, Some(&old.metadata)).unwrap();
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
        super::RecordingSessionContext::new("N64 Capture".to_owned(), "en".to_owned(), None),
        catalog.clone(),
    );
    let now = Instant::now();
    recording.tracker.status = Some(RunStatus::Complete);
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
        output_policy: &options.output_policy(),
        recent_run_limit: options.recent_run_limit,
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
    recording.tracker.pending = None;
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

    recording.tracker.pending = None;
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
    recording.tracker.pending = None;
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
    recording.tracker.pending = None;
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
    recording.tracker.pending = None;
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
    recording.tracker.pending = None;
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
    recording.tracker.pending = None;
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
    recording.tracker.pending = None;
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
    assert_eq!(fire_at, stats_at + recording.tracker_policy.save_delay());
    recording.poll_pending(fire_at - Duration::from_millis(1));
    assert!(recording.tracker.pending.is_some());
    recording.tracker.pending = None;
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
    assert_eq!(recording.tracker.status, Some(RunStatus::Complete));
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
        assert_eq!(recording.tracker.clip_start, None);
        assert_eq!(recording.tracker.status, None);
        assert!(recording.tracker.report.is_none());
        assert!(recording.tracker.pending.is_none());
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
    assert_eq!(recording.tracker.clip_start, Some(start));

    recording.on_frame(stats_at, &match_with_time());

    let pending = pending_save_event(&mut events);
    assert_eq!(pending.status, "complete");
    assert!((pending.estimated_duration_secs - 31.0).abs() < f64::EPSILON);

    let job = recording.take_pending_job(stats_at + Duration::from_secs(5)).expect("save job");
    assert_eq!(job.status, RunStatus::Complete);
    assert!((job.start_before_save_secs - 30.5).abs() < f64::EPSILON);
}

#[test]
fn seven_options_launches_and_canonicalizes_the_completed_run() {
    let (mut recording, mut events) = test_recording_saving_short_failed_runs();
    let start = Instant::now();

    recording.on_frame(start, &match_for_screen_with_identity(Screen::Opts007, 1, 1, 3));
    recording.on_frame(start + Duration::from_secs(10), &stats_match(14));

    let pending = pending_save_event(&mut events);
    let stats = recording.tracker.pending.as_ref().and_then(|pending| pending.stats.as_ref()).expect("pending stats");
    assert_eq!((stats.mission, stats.part, stats.difficulty), (1, 1, 3));
    assert_eq!(pending.difficulty.as_deref(), Some("007"));
    recording.tracker.pending = None;
}

#[test]
fn returning_to_difficulty_selection_cancels_a_007_launch() {
    let (mut recording, _events) = test_recording(RecordingOptions::default());
    let start = Instant::now();

    recording.on_frame(start, &match_for_screen_with_identity(Screen::Opts007, 1, 1, 3));
    recording.on_frame(start + Duration::from_secs(1), &match_for_screen(Screen::Select));

    assert_eq!(recording.tracker.clip_start, None);
    assert_eq!(recording.tracker.status, None);
    assert_eq!(recording.tracker.identity_vote.winner, None);
    assert_eq!(recording.recording_state.current(), Some(RecordingStatus::Cancelled));
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

        assert_eq!(recording.tracker.status, Some(run_status));
        assert_eq!(recording.tracker.report.as_ref().map(|m| m.screen), Some(screen));
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
fn run_tracker_reports_start_without_application_dependencies() {
    let mut tracker = RunTracker::new("en".to_owned());
    let now = Instant::now();

    let update = tracker.on_frame(
        now,
        UNIX_EPOCH,
        &match_for_screen(Screen::Start),
        RecordingOptions::default().tracker_policy(),
    );

    assert!(update.ensure_replay_buffer);
    assert_eq!(update.phase, Some(RecordingStatus::Started));
    assert_eq!(tracker.clip_start, Some(now));
    assert!(update.ready.is_empty());
}

#[test]
fn run_tracker_schedules_a_completed_run_as_a_domain_transition() {
    let mut tracker = RunTracker::new("en".to_owned());
    let options = RecordingOptions::default();
    let start = Instant::now();
    let finish = start + Duration::from_secs(12);
    let policy = options.tracker_policy();
    tracker.on_frame(start, UNIX_EPOCH, &match_for_screen(Screen::Start), policy);
    tracker.on_frame(
        start + Duration::from_secs(10),
        UNIX_EPOCH,
        &match_for_screen(Screen::Complete),
        policy,
    );

    let update = tracker.on_frame(finish, UNIX_EPOCH + Duration::from_secs(12), &match_with_time(), policy);

    assert_eq!(update.phase, Some(RecordingStatus::SavePending));
    assert!(update.pending_changed);
    let pending = tracker.pending.as_ref().expect("scheduled save");
    assert_eq!(pending.status, RunStatus::Complete);
    assert_eq!(pending.completed_at, UNIX_EPOCH + Duration::from_secs(12));
    assert_eq!(pending.fire_at, finish + policy.save_delay());
}
