#[test]
fn replay_save_wait_keeps_ownership_after_the_slow_warning() {
    let since = begin_replay_save_request();
    let publisher = std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(30));
        on_replay_saved(Some("/replays/late.mp4".to_owned()));
    });

    let result = wait_for_replay_saved(since, Duration::from_millis(5), Duration::from_secs(1));
    publisher.join().unwrap();

    assert_eq!(result, ReplaySaveWait::Saved(Some("/replays/late.mp4".to_owned())));
}
