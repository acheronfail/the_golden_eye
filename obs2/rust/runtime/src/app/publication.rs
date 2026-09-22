use serde::Serialize;
use tokio::sync::watch;

use crate::cv::LevelMatch;
use crate::obs::{ReplayBufferStatus, Source};
use crate::run_library::RunCatalogSync;
use crate::run_monitoring::RecordingStatus;
use crate::run_monitoring::publication::*;

#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub monitor: MonitorSnapshot,
    #[serde(rename = "match")]
    pub level_match: Option<LevelMatch>,
    pub run_catalog_sync: Option<RunCatalogSync>,
    pub recording_state: Option<RecordingStatus>,
    pub replay_saves: Vec<ReplaySaveStatus>,
    pub sources: Vec<Source>,
    pub replay_buffer: ReplayBufferStatus,
    pub settings_status: crate::settings::SettingsStatus,
    pub update: crate::plugin_updates::UpdateStatus,
}

#[derive(Clone)]
pub struct SharedStateStore {
    tx: watch::Sender<AppSnapshot>,
}

impl SharedStateStore {
    pub fn new(initial: AppSnapshot) -> Self {
        let (tx, _) = watch::channel(initial);
        Self { tx }
    }

    pub fn subscribe(&self) -> watch::Receiver<AppSnapshot> {
        self.tx.subscribe()
    }

    #[cfg(test)]
    pub fn current(&self) -> AppSnapshot {
        self.tx.borrow().clone()
    }

    pub fn set_run_catalog_sync(&self, run_catalog_sync: Option<RunCatalogSync>) {
        self.update(|state| state.run_catalog_sync = run_catalog_sync);
    }

    pub fn set_sources(&self, sources: Vec<Source>) {
        self.update(|state| state.sources = sources);
    }

    pub fn set_replay_buffer(&self, replay_buffer: ReplayBufferStatus) {
        self.update(|state| state.replay_buffer = replay_buffer);
    }

    pub fn set_settings_status(&self, settings_status: crate::settings::SettingsStatus) {
        self.update(|state| state.settings_status = settings_status);
    }

    pub fn current_update_status(&self) -> crate::plugin_updates::UpdateStatus {
        self.tx.borrow().update.clone()
    }

    /// Mutate and publish the same retained value; no second snapshot can lag behind.
    pub(crate) fn update(&self, apply: impl FnOnce(&mut AppSnapshot)) {
        self.tx.send_if_modified(|state| {
            let previous = state.clone();
            apply(state);
            *state != previous
        });
    }
}

#[cfg(test)]
#[path = "publication_test.rs"]
mod tests;
