//! Applies settings changes to the connected feature owners and client publication.
use std::time::Duration;

use serde_json::Value;

use super::{AppEvent, AppState, AppStateInner};
use crate::settings::{AppSettings, SettingsReload};

impl AppStateInner {
    pub(crate) fn save_settings(&self, value: Value) -> anyhow::Result<AppSettings> {
        let settings = self.settings.set_from_json_value_with_runtime_defaults(value)?;
        self.settings_changed(&settings);
        Ok(settings)
    }

    pub(crate) fn reset_settings(&self) -> anyhow::Result<AppSettings> {
        let settings = self.settings.reset_to_defaults()?;
        self.settings_changed(&settings);
        Ok(settings)
    }

    fn settings_changed(&self, settings: &AppSettings) {
        self.monitor.set_recent_run_limit(settings.recent_run_limit);
        self.snapshot.set_settings_status(self.settings.status());
        let _ = self.event_tx.send(AppEvent::RunCatalogChanged { run_id: None, save_id: None });
    }
}

pub(crate) async fn watch_settings_file(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        match state.settings.reload_from_disk_if_changed() {
            SettingsReload::Unchanged => {}
            SettingsReload::Reloaded(settings) => {
                state.snapshot.set_settings_status(state.settings.status_without_runtime_defaults());
                let _ = state.event_tx.send(AppEvent::SettingsReloaded {
                    config_path: state.settings.path().to_string_lossy().into_owned(),
                    settings: *settings,
                });
            }
            SettingsReload::Invalid(error) => {
                state.snapshot.set_settings_status(state.settings.status_without_runtime_defaults());
                let _ = state.event_tx.send(AppEvent::SettingsInvalid {
                    config_path: state.settings.path().to_string_lossy().into_owned(),
                    error,
                });
            }
        }
    }
}
