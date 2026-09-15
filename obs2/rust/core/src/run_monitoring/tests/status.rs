use super::*;
use crate::run_monitoring::test_support::test_snapshot_store;

#[test]
fn recording_state_store_updates_snapshot_without_receivers() {
    let snapshot = test_snapshot_store();
    let rx = snapshot.subscribe();
    let store = RecordingStateStore::new(snapshot.clone());
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
    let store = RecordingStateStore::new(snapshot.clone());
    let first = store.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::Started));
    let second = store.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::Started));
    store.handle(crate::run_monitoring::RecordingStateEvent::SaveFinished(first));
    assert_eq!(store.current(), Some(RecordingStatus::Started));
    assert_eq!(snapshot.current().recording_state, Some(RecordingStatus::Started));
    store.handle(crate::run_monitoring::RecordingStateEvent::SaveFinished(second));
    assert_eq!(store.current(), None);
    assert_eq!(snapshot.current().recording_state, None);
}

#[tokio::test]
async fn cancelled_phase_expires_and_publishes_the_cleared_status() {
    let snapshot = test_snapshot_store();
    let store = RecordingStateStore::new(snapshot.clone());
    let mut updates = snapshot.subscribe();
    let started_at = std::time::Instant::now();
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
fn expiry_from_before_a_reset_cannot_clear_the_new_session() {
    let snapshot = test_snapshot_store();
    let store = RecordingStateStore::new(snapshot.clone());
    let old = store.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::Started));
    store.handle(RecordingStateEvent::Reset);
    let current = store.handle(RecordingStateEvent::PhaseChanged(RecordingStatus::Started));
    store.handle(RecordingStateEvent::Expired(old));
    assert_eq!(snapshot.current().recording_state, Some(RecordingStatus::Started));
    store.handle(RecordingStateEvent::Expired(current));
    assert_eq!(snapshot.current().recording_state, None);
}
