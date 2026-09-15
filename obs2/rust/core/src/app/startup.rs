//! Constructs feature owners and connects their shared dependencies once per core load.
use std::sync::Arc;

use super::{AppSnapshot, AppState, AppStateInner, SharedStateStore};
use crate::db::run_catalog::RunCatalog;
use crate::run_monitoring::RecordingStateStore;
use crate::run_monitoring::publication::MonitorSnapshot;
use crate::settings::SettingsStore;
use crate::{
    capture_tools,
    obs,
    plugin_updates,
    run_library,
    run_monitoring,
    streaming_notifications,
    youtube_uploads,
};

pub(crate) fn build_state(
    settings: Arc<SettingsStore>,
    run_catalog: Arc<RunCatalog>,
    catalog_needs_seed: bool,
    was_reloaded: bool,
) -> AppState {
    let snapshot = SharedStateStore::new(AppSnapshot {
        monitor: MonitorSnapshot {
            enabled: false,
            source_name: None,
            cv_language: None,
            wall_clocks: run_monitoring::publication::MonitorWallClockState::default(),
        },
        level_match: None,
        run_catalog_sync: None,
        recording_state: None,
        replay_saves: Vec::new(),
        sources: Vec::new(),
        replay_buffer: obs::ReplayBufferStatus::unknown(),
        settings_status: settings.status_without_runtime_defaults(),
        // During a reload the shim removes the consumed staged directory only
        // after this new core starts, so it must not be advertised as pending.
        update: initial_update_status(was_reloaded, plugin_updates::installation::has_staged_update()),
    });
    // One-off monitor events (recording saved, ...). Capacity bounds how far a
    // slow client can lag before it drops events; the worker ignores send errors,
    // so a full/empty channel never blocks frame processing.
    let (event_tx, _) = tokio::sync::broadcast::channel(64);
    let (frontend_ready_tx, _) = tokio::sync::watch::channel(was_reloaded);
    let recording_state = RecordingStateStore::new(snapshot.clone());
    let replay_saves = run_monitoring::publication::ReplaySaveStateStore::new(snapshot.clone());
    let monitor = Arc::new(run_monitoring::RunMonitor::new(
        settings.clone(),
        run_catalog.clone(),
        snapshot.clone(),
        event_tx.clone(),
        recording_state.clone(),
        replay_saves,
    ));
    let updates = Arc::new(plugin_updates::PluginUpdates::new(
        settings.clone(),
        snapshot.clone(),
        event_tx.clone(),
        monitor.clone(),
        recording_state,
        frontend_ready_tx.clone(),
    ));
    Arc::new(AppStateInner {
        youtube: youtube_uploads::YoutubeUploadStore::new(settings.path(), run_catalog.clone()),
        notifications: streaming_notifications::StreamNotifier::new(settings.clone()),
        monitor: monitor.clone(),
        updates,
        snapshot: snapshot.clone(),
        event_tx: event_tx.clone(),
        frame_dump: capture_tools::FrameDumper::new(monitor),
        frontend_ready_tx,
        run_catalog: run_catalog.clone(),
        runs: Arc::new(run_library::RunLibrary::new(
            run_catalog.clone(),
            catalog_needs_seed,
            snapshot.clone(),
            event_tx.clone(),
        )),
        settings,
        reloaded_at: was_reloaded.then(std::time::Instant::now),
    })
}

fn initial_update_status(was_reloaded: bool, staged_update_present: bool) -> plugin_updates::UpdateStatus {
    if !was_reloaded && staged_update_present {
        plugin_updates::UpdateStatus { phase: plugin_updates::UpdatePhase::Staged, available: None }
    } else {
        plugin_updates::UpdateStatus::default()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reload_does_not_advertise_the_consumed_staged_update() {
        let status = super::initial_update_status(true, true);
        assert_eq!(status.phase, crate::plugin_updates::UpdatePhase::Idle);
        assert!(status.available.is_none());
    }

    #[test]
    fn cold_start_advertises_an_existing_staged_update() {
        let status = super::initial_update_status(false, true);
        assert_eq!(status.phase, crate::plugin_updates::UpdatePhase::Staged);
        assert!(status.available.is_none());
    }
}
