use super::*;
use crate::run_monitoring::test_support::test_snapshot_store;

#[test]
fn recording_state_store_updates_snapshot_without_receivers() {
    let snapshot = test_snapshot_store();
    let rx = snapshot.subscribe();
    let store = RecordingStateStore::new(snapshot.clone(), crate::run_monitoring::test_support::test_runtime_handle());
    drop(rx);

    store.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::Started));
    assert_eq!(store.current(), Some(RecordingStatus::Started));
    assert_eq!(snapshot.current().recording_state, Some(RecordingStatus::Started));

    // A stale generation (superseded by a later transition) must not clear
    // the phase, even though its captured value matches the current one.
    let stale_generation = store.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::SavePending));
    store.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::Started));
    store.handle(crate::run_monitoring::RecordingStateEvent::SaveFinished(stale_generation));
    assert_eq!(store.current(), Some(RecordingStatus::Started));

    // The current generation clears normally.
    let current_generation = store.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::SavePending));
    store.handle(crate::run_monitoring::RecordingStateEvent::SaveFinished(current_generation));
    assert_eq!(store.current(), None);

    store.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::Started));
    store.handle(RecordingStateEvent::Reset);
    assert_eq!(store.current(), None);
    assert_eq!(snapshot.current().recording_state, None);
}

#[test]
fn stale_completion_cannot_clear_a_newer_identical_status() {
    let snapshot = test_snapshot_store();
    let store = RecordingStateStore::new(snapshot.clone(), crate::run_monitoring::test_support::test_runtime_handle());
    let first = store.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::Started));
    let second = store.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::Started));
    store.handle(crate::run_monitoring::RecordingStateEvent::SaveFinished(first));
    assert_eq!(store.current(), Some(RecordingStatus::Started));
    assert_eq!(snapshot.current().recording_state, Some(RecordingStatus::Started));
    store.handle(crate::run_monitoring::RecordingStateEvent::SaveFinished(second));
    assert_eq!(store.current(), None);
    assert_eq!(snapshot.current().recording_state, None);
}

#[tokio::test(start_paused = true)]
async fn cancelled_phase_expires_and_publishes_the_cleared_status() {
    let snapshot = test_snapshot_store();
    let store = RecordingStateStore::new(snapshot.clone(), crate::run_monitoring::test_support::test_runtime_handle());
    let mut updates = snapshot.subscribe();
    let started_at = tokio::time::Instant::now();
    store.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::Cancelled));
    assert_eq!(updates.borrow_and_update().recording_state, Some(RecordingStatus::Cancelled));
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            updates.changed().await.expect("status publisher remains alive");
            if updates.borrow_and_update().recording_state.is_none() {
                break;
            }
        }
    })
    .await
    .expect("cancelled status should expire");
    assert!(started_at.elapsed() >= RecordingStateStore::CANCELLED_LINGER);
    assert_eq!(store.current(), None);
    assert_eq!(snapshot.current().recording_state, None);
}

#[test]
fn runtime_shutdown_drops_pending_expiry_work_from_native_threads() {
    use crate::run_monitoring::publication::{ReplaySaveStage, ReplaySaveStateStore, ReplaySaveStatus};

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let snapshot = test_snapshot_store();
    let updates = snapshot.subscribe();
    let recording = RecordingStateStore::new(snapshot.clone(), runtime.handle().clone());
    let saves = ReplaySaveStateStore::new(snapshot.clone(), runtime.handle().clone());
    drop(snapshot);

    // Production schedules these from monitor/save threads without a current runtime.
    std::thread::spawn(move || {
        recording.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::SavePending));
        for tracking_id in [1, 2] {
            saves.schedule(ReplaySaveStatus {
                tracking_id,
                save_id: tracking_id,
                stage: ReplaySaveStage::Scheduled,
                level: "Runway".to_owned(),
                difficulty: None,
                run_status: "complete".to_owned(),
                estimated_duration_secs: 28.0,
                error: None,
            });
        }
        saves.complete(1);
        saves.fail(2, "test failure".to_owned());
    })
    .join()
    .unwrap();

    assert!(updates.has_changed().is_ok(), "pending timers retain the publisher");
    drop(runtime);
    assert!(updates.has_changed().is_err(), "no timer may survive core runtime shutdown");
}

#[test]
fn expiry_from_before_a_reset_cannot_clear_the_new_session() {
    let snapshot = test_snapshot_store();
    let store = RecordingStateStore::new(snapshot.clone(), crate::run_monitoring::test_support::test_runtime_handle());
    let old = store.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::Started));
    store.handle(RecordingStateEvent::Reset);
    let current = store.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::Started));
    store.handle(RecordingStateEvent::Expired(old));
    assert_eq!(snapshot.current().recording_state, Some(RecordingStatus::Started));
    store.handle(RecordingStateEvent::Expired(current));
    assert_eq!(snapshot.current().recording_state, None);
}
