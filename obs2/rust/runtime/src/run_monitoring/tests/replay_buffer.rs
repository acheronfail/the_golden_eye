use super::*;

#[test]
fn shutdown_cancels_save_waits_and_rejects_new_requests() {
    let coordinator = ReplayCoordinator::new();
    std::thread::scope(|scope| {
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let coordinator = &coordinator;
        let worker = scope.spawn(move || {
            coordinator.acquire_save().save_and_wait(
                || started_tx.send(()).unwrap(),
                Duration::from_secs(30),
                Duration::from_secs(120),
            )
        });
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        coordinator.begin_shutdown();
        assert_eq!(worker.join().unwrap(), ReplaySaveWait::ShuttingDown);
        let result = coordinator.acquire_save().save_and_wait(
            || panic!("must not call OBS after shutdown"),
            Duration::ZERO,
            Duration::ZERO,
        );
        assert_eq!(result, ReplaySaveWait::ShuttingDown);
        coordinator.handle(ReplayEvent::Saved(Some("raw-replay.mp4".to_owned())));
        assert_eq!(coordinator.saved.lock().unwrap().pending_requests, 0);
        assert_eq!(coordinator.saved.lock().unwrap().generation, 0);
    });
}

#[test]
fn shutdown_interrupts_padding_and_replay_lifecycle_waits() {
    let coordinator = ReplayCoordinator::new();
    coordinator.handle(ReplayEvent::Stopping);
    std::thread::scope(|scope| {
        let padding = scope.spawn(|| coordinator.wait_for_padding(Duration::from_secs(120)));
        let stopping = scope.spawn(|| coordinator.wait_for_replay_buffer_not_stopping(Duration::from_secs(120)));
        coordinator.begin_shutdown();
        padding.join().unwrap();
        assert!(!stopping.join().unwrap());
    });
    coordinator.reopen();
    assert!(coordinator.wait_for_replay_buffer_not_stopping(Duration::ZERO));
}

#[test]
fn replay_save_wait_keeps_ownership_after_the_slow_warning() {
    let coordinator = ReplayCoordinator::new();
    std::thread::scope(|scope| {
        let mut save = coordinator.acquire_save();
        let result = save.save_and_wait(
            || {
                scope.spawn(|| {
                    std::thread::sleep(Duration::from_millis(30));
                    coordinator.handle(ReplayEvent::Saved(Some("/replays/late.mp4".to_owned())));
                });
            },
            Duration::from_millis(5),
            Duration::from_secs(1),
        );
        assert_eq!(result, ReplaySaveWait::Saved(Some("/replays/late.mp4".to_owned())));
    });
}

#[test]
fn immediate_completion_is_registered_before_the_obs_call() {
    let coordinator = ReplayCoordinator::new();
    coordinator.handle(ReplayEvent::Saved(Some("/replays/manual.mp4".to_owned())));
    let mut save = coordinator.acquire_save();
    let result = save.save_and_wait(
        || coordinator.handle(ReplayEvent::Saved(Some("/replays/plugin.mp4".to_owned()))),
        Duration::ZERO,
        Duration::ZERO,
    );
    assert_eq!(result, ReplaySaveWait::Saved(Some("/replays/plugin.mp4".to_owned())));
}

#[test]
fn timeout_releases_request_ownership_and_ignores_later_manual_saves() {
    let coordinator = ReplayCoordinator::new();
    let mut save = coordinator.acquire_save();
    assert_eq!(save.save_and_wait(|| {}, Duration::ZERO, Duration::ZERO), ReplaySaveWait::TimedOut);
    coordinator.handle(ReplayEvent::Saved(Some("/replays/manual.mp4".to_owned())));
    // An unrelated callback must not supply a completion to a later plugin save.
    assert_eq!(save.save_and_wait(|| {}, Duration::ZERO, Duration::ZERO), ReplaySaveWait::TimedOut);
    assert_eq!(coordinator.saved.lock().unwrap().generation, 0);
    assert_eq!(coordinator.saved.lock().unwrap().pending_requests, 0);
}

#[test]
fn save_permit_serializes_through_file_identification_then_releases() {
    let coordinator = ReplayCoordinator::new();
    std::thread::scope(|scope| {
        let (attempt_tx, attempt_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let mut first = coordinator.acquire_save();
        assert_eq!(
            first.save_and_wait(
                || coordinator.handle(ReplayEvent::Saved(Some("first.mp4".to_owned()))),
                Duration::ZERO,
                Duration::ZERO,
            ),
            ReplaySaveWait::Saved(Some("first.mp4".to_owned()))
        );
        let coordinator = &coordinator;
        scope.spawn(move || {
            attempt_tx.send(()).unwrap();
            let mut second = coordinator.acquire_save();
            let result = second.save_and_wait(
                || coordinator.handle(ReplayEvent::Saved(Some("second.mp4".to_owned()))),
                Duration::ZERO,
                Duration::ZERO,
            );
            done_tx.send(result).unwrap();
        });
        attempt_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(done_rx.recv_timeout(Duration::from_millis(20)).is_err());
        drop(first);
        assert_eq!(
            done_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            ReplaySaveWait::Saved(Some("second.mp4".to_owned()))
        );
    });
}

#[test]
fn stopping_blocks_restart_and_stopped_retains_the_settle_deadline() {
    let coordinator = ReplayCoordinator::new();
    coordinator.handle(ReplayEvent::Starting);
    coordinator.handle(ReplayEvent::Started);
    coordinator.handle(ReplayEvent::Stopping);
    assert!(!coordinator.wait_for_replay_buffer_not_stopping(Duration::ZERO));
    let before_stop = Instant::now();
    coordinator.handle(ReplayEvent::Stopped);
    let stopped_at = coordinator.lifecycle.lock().unwrap().last_stopped_at.unwrap();
    assert!(stopped_at >= before_stop);
    assert!(coordinator.wait_for_replay_buffer_not_stopping(Duration::ZERO));
    assert!(stopped_at.elapsed() >= REPLAY_STOP_SETTLE_DELAY);
    coordinator.handle(ReplayEvent::Starting);
    coordinator.handle(ReplayEvent::Started);
    assert!(coordinator.lifecycle.lock().unwrap().last_stopped_at.is_none());
}
