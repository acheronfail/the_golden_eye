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
    assert_eq!(job.metadata.game_language, "en");
    assert_eq!(job.metadata.rom_version, None);
    assert_eq!(job.event_tx.receiver_count(), 1);
    assert_eq!(job.recording_state.current(), None);
    assert!((job.start_before_save_secs - 17.0).abs() < f64::EPSILON);
    assert_eq!(job.trim_tail_secs, 0.0);
    assert!(recording.tracker.pending.is_none());
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
    assert!(recording.tracker.pending.is_none());
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

    let new_files = new_replay_files(dir.path(), &before, Some(&added.to_string_lossy()));

    // Only the newly-added file with the saved file's extension counts: the
    // pre-existing file and the unrelated `.txt` are both excluded.
    assert_eq!(new_files, vec![added]);
}

#[test]
fn resolve_saved_replay_trusts_the_single_new_file_over_the_event_path() {
    let event_path = "/replays/user-save.mp4".to_owned();
    let ours = PathBuf::from("/replays/our-save.mp4");

    let resolved = resolve_saved_replay(Some(event_path), vec![ours.clone()]).unwrap();

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
    let resolved = resolve_saved_replay(Some(event_path.clone()), vec![a, b]).unwrap();
    assert_eq!(resolved.path, event_path);
    assert!(!resolved.safe_to_delete);

    // No new file at all is treated the same conservative way.
    let resolved = resolve_saved_replay(Some(event_path.clone()), vec![]).unwrap();
    assert_eq!(resolved.path, event_path);
    assert!(!resolved.safe_to_delete);
}

#[test]
fn resolve_saved_replay_recovers_a_single_new_file_when_obs_omits_the_path() {
    let ours = PathBuf::from("/replays/our-save.mp4");

    let resolved = resolve_saved_replay(None, vec![ours.clone()]).unwrap();

    assert_eq!(resolved.path, ours.to_string_lossy());
    assert!(resolved.safe_to_delete);
    assert!(resolve_saved_replay(None, vec![]).is_none());
}
