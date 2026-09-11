use super::*;

#[test]
fn replay_save_wait_keeps_ownership_after_the_slow_warning() {
    let coordinator = ReplayCoordinator::new();
    std::thread::scope(|scope| {
        let mut save = coordinator.acquire_save();
        let result = save.save_and_wait(
            || {
                scope.spawn(|| {
                    std::thread::sleep(Duration::from_millis(30));
                    coordinator.on_replay_saved(Some("/replays/late.mp4".to_owned()));
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
    coordinator.on_replay_saved(Some("/replays/manual.mp4".to_owned()));
    let mut save = coordinator.acquire_save();
    let result = save.save_and_wait(
        || coordinator.on_replay_saved(Some("/replays/plugin.mp4".to_owned())),
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
    coordinator.on_replay_saved(Some("/replays/manual.mp4".to_owned()));
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
                || coordinator.on_replay_saved(Some("first.mp4".to_owned())),
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
                || coordinator.on_replay_saved(Some("second.mp4".to_owned())),
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
    coordinator.on_replay_buffer_starting();
    coordinator.on_replay_buffer_started();
    coordinator.on_replay_buffer_stopping();
    assert!(!coordinator.wait_for_replay_buffer_not_stopping(Duration::ZERO));
    let before_stop = Instant::now();
    coordinator.on_replay_buffer_stopped();
    let stopped_at = coordinator.lifecycle.lock().unwrap().last_stopped_at.unwrap();
    assert!(stopped_at >= before_stop);
    assert!(coordinator.wait_for_replay_buffer_not_stopping(Duration::ZERO));
    assert!(stopped_at.elapsed() >= REPLAY_STOP_SETTLE_DELAY);
    coordinator.on_replay_buffer_starting();
    coordinator.on_replay_buffer_started();
    assert!(coordinator.lifecycle.lock().unwrap().last_stopped_at.is_none());
}
