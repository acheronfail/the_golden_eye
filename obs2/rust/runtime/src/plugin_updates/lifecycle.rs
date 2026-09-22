//! Update workflow: check, stage, publish progress, and apply when activity allows.
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use tokio::sync::{Mutex, broadcast, watch};

use super::installation::{has_staged_update, trigger_apply};
use super::releases::{fetch_latest_update, is_check_due, now_unix_seconds};
use super::{DownloadUpdateResult, PluginUpdate, UpdatePhase, UpdateStatus};
use crate::app::{AppEvent, SharedStateStore};
use crate::run_monitoring::{RecordingStateStore, RunMonitor};
use crate::settings::SettingsStore;

const AUTO_APPLY_CHECK_INTERVAL: Duration = Duration::from_secs(30);

pub(crate) struct PluginUpdates {
    settings: Arc<SettingsStore>,
    snapshot: SharedStateStore,
    event_tx: broadcast::Sender<AppEvent>,
    monitor: Arc<RunMonitor>,
    recording_state: RecordingStateStore,
    frontend_ready_tx: watch::Sender<bool>,
    check_lock: Mutex<()>,
}

#[derive(Debug)]
pub(crate) enum ApplyError {
    NothingStaged,
    ActivityInProgress,
}

impl PluginUpdates {
    pub(crate) fn new(
        settings: Arc<SettingsStore>,
        snapshot: SharedStateStore,
        event_tx: broadcast::Sender<AppEvent>,
        monitor: Arc<RunMonitor>,
        recording_state: RecordingStateStore,
        frontend_ready_tx: watch::Sender<bool>,
    ) -> Self {
        Self { settings, snapshot, event_tx, monitor, recording_state, frontend_ready_tx, check_lock: Mutex::new(()) }
    }

    pub(crate) fn status(&self) -> UpdateStatus {
        self.snapshot.current_update_status()
    }

    fn publish(&self, status: UpdateStatus) {
        self.snapshot.update(|snapshot| snapshot.update = status);
    }

    pub(crate) fn apply_now(&self) -> Result<(), ApplyError> {
        if !has_staged_update() {
            return Err(ApplyError::NothingStaged);
        }
        if !self.is_safe_to_apply() {
            return Err(ApplyError::ActivityInProgress);
        }
        let status = self.status();
        self.publish(UpdateStatus { phase: UpdatePhase::Applying, available: status.available });
        trigger_apply();
        Ok(())
    }

    pub async fn check_for_updates_on_startup(self: Arc<Self>) {
        // Dev builds restart the server on every hot reload, which would re-hit GitHub's
        // API each time (`last_update_check_time` only advances on success), so a
        // rate-limited dev session would keep retrying. No reason to check locally anyway.
        if cfg!(feature = "dev") {
            tracing::debug!("skipping plugin update check in a dev build");
            return;
        }
        if crate::plugin_updates::installation::has_staged_update() {
            tracing::debug!("skipping plugin update check while an update is staged");
            return;
        }

        let settings = self.settings.get();
        if !is_check_due(settings.update_check_interval, settings.last_update_check_time, now_unix_seconds()) {
            tracing::debug!("plugin update check not due");
            return;
        }

        if let Err(err) = self.check_for_updates_now().await {
            tracing::warn!("plugin update check failed: {err:#}");
        }
    }

    /// Checks for an update now, bypassing the configured interval and dev skip. Shared by
    /// the startup check and the manual "check now" endpoint. Records the check time, pushes
    /// the retained app snapshot, and (if opted in) stages in the background. `Ok(None)` if up to date.
    pub async fn check_for_updates_now(self: Arc<Self>) -> anyhow::Result<Option<PluginUpdate>> {
        let _check_guard = self.check_lock.lock().await;
        let current = self.status();
        if matches!(current.phase, UpdatePhase::Downloading | UpdatePhase::Staged | UpdatePhase::Applying) {
            return Ok(current.available);
        }
        self.publish(UpdateStatus { phase: UpdatePhase::Checking, available: current.available.clone() });

        if self.settings.status_without_runtime_defaults().file_error.is_some() {
            tracing::info!("settings file is invalid; skipping plugin update check");
            self.publish(UpdateStatus {
                phase: if current.available.is_some() { UpdatePhase::Available } else { UpdatePhase::Idle },
                available: current.available,
            });
            return Ok(None);
        }

        let checked_at = now_unix_seconds();
        let found = match fetch_latest_update(crate::PLUGIN_VERSION).await {
            Ok(found) => found,
            Err(err) => {
                let current = self.status();
                self.publish(UpdateStatus {
                    phase: if current.available.is_some() { UpdatePhase::Available } else { UpdatePhase::Idle },
                    available: current.available,
                });
                return Err(err);
            }
        };
        if let Err(err) = self.settings.set_last_update_check_time(checked_at).context("saving last update check time")
        {
            let current = self.status();
            self.publish(UpdateStatus {
                phase: if current.available.is_some() { UpdatePhase::Available } else { UpdatePhase::Idle },
                available: current.available,
            });
            return Err(err);
        }
        self.snapshot.set_settings_status(self.settings.status_without_runtime_defaults());

        let Some((update, assets)) = found else {
            tracing::info!(version = crate::PLUGIN_VERSION, "plugin is up to date");
            self.publish(UpdateStatus::default());
            return Ok(None);
        };

        tracing::info!(
            current_version = %update.current_version,
            latest_version = %update.latest_version,
            updater_version = update.updater_version,
            requires_manual_install = update.requires_manual_install,
            release_url = %update.release_url,
            "plugin update available"
        );
        let auto_update_enabled = self.settings.get().auto_update_enabled;
        self.publish(UpdateStatus {
            phase: if auto_update_enabled && !update.requires_manual_install {
                UpdatePhase::Downloading
            } else {
                UpdatePhase::Available
            },
            available: Some(update.clone()),
        });

        // Best-effort: this only feeds the "click to view the changelog" link on
        // the later "plugin updated" notice (see `routes::monitor::handle_socket`),
        // so a persistence failure here shouldn't fail the update check itself.
        if let Err(err) = self.settings.set_last_known_update(&update.latest_version, &update.release_url) {
            tracing::warn!("failed to persist last known plugin update: {err:#}");
        }

        // Only download/stage automatically when opted into auto installs. Otherwise the
        // "Download now" button / notice (via `download_and_stage_latest`) drives it on an
        // explicit click, so we don't fetch a release the user hasn't asked for.
        if auto_update_enabled && !update.requires_manual_install {
            // Reuses this same fetch's asset list rather than fetching the release
            // again, which would double GitHub API traffic for every check.
            let update_for_stage = update.clone();
            let state_for_stage = self.clone();
            let event_tx = self.event_tx.clone();
            tokio::spawn(async move {
                if let Err(err) =
                    crate::plugin_updates::installation::download_verify_and_stage(&update_for_stage, assets).await
                {
                    tracing::error!("failed to stage plugin update: {err:#}");
                    state_for_stage
                        .publish(UpdateStatus { phase: UpdatePhase::Available, available: Some(update_for_stage) });
                    let _ = event_tx.send(crate::app::AppEvent::UpdateStagingFailed { error: format!("{err:#}") });
                } else {
                    state_for_stage
                        .publish(UpdateStatus { phase: UpdatePhase::Staged, available: Some(update_for_stage) });
                    if state_for_stage.settings.get().auto_update_enabled {
                        state_for_stage.trigger_apply_if_safe();
                    }
                }
            });
        }

        Ok(Some(update))
    }

    /// Fetches the latest release and, if compatible, downloads/verifies/stages it,
    /// blocking until staging finishes. Explicit downloads bypass the auto-update
    /// preference but never the updater-version compatibility gate.
    pub async fn download_and_stage_latest(self: Arc<Self>) -> anyhow::Result<DownloadUpdateResult> {
        let previous = self.status();
        if matches!(previous.phase, UpdatePhase::Staged | UpdatePhase::Applying) {
            return Ok(DownloadUpdateResult::Staged);
        }
        self.publish(UpdateStatus { phase: UpdatePhase::Downloading, available: previous.available.clone() });
        let found = match fetch_latest_update(crate::PLUGIN_VERSION).await {
            Ok(found) => found,
            Err(err) => {
                self.publish(UpdateStatus {
                    phase: if previous.available.is_some() { UpdatePhase::Available } else { UpdatePhase::Idle },
                    available: previous.available,
                });
                return Err(err);
            }
        };
        let Some((update, assets)) = found else {
            self.publish(UpdateStatus::default());
            return Ok(DownloadUpdateResult::UpToDate);
        };
        if update.requires_manual_install {
            self.publish(UpdateStatus { phase: UpdatePhase::Available, available: Some(update) });
            return Ok(DownloadUpdateResult::ManualInstallRequired);
        }
        self.publish(UpdateStatus { phase: UpdatePhase::Downloading, available: Some(update.clone()) });
        if let Err(err) = crate::plugin_updates::installation::download_verify_and_stage(&update, assets).await {
            self.publish(UpdateStatus { phase: UpdatePhase::Available, available: Some(update) });
            return Err(err);
        }
        self.publish(UpdateStatus { phase: UpdatePhase::Staged, available: Some(update) });
        Ok(DownloadUpdateResult::Staged)
    }

    /// Production requires monitoring and recording to be idle; dev allows hot reload.
    /// Recheck at application time. The OBS-owned replay buffer can remain running.
    pub fn is_safe_to_apply(&self) -> bool {
        let monitor_active = self.monitor.is_active();
        let recording_active = self.recording_state.current().is_some();
        activity_is_safe_to_apply(monitor_active, recording_active)
    }

    /// Applies a staged update immediately when the frontend is ready and runtime
    /// activity is safe. Returns whether a reload was requested.
    pub fn trigger_apply_if_safe(&self) -> bool {
        if !*self.frontend_ready_tx.borrow() || !has_staged_update() || !self.is_safe_to_apply() {
            return false;
        }
        let status = self.status();
        self.publish(crate::plugin_updates::UpdateStatus {
            phase: crate::plugin_updates::UpdatePhase::Applying,
            available: status.available,
        });
        trigger_apply();
        true
    }

    /// Background task: periodically applies a staged update when opted in
    /// (`autoUpdateEnabled`) and safe. Spawned once from `ge_runtime_start`. Dev builds
    /// always count as opted in and poll faster (hot-reload fallback for `just dev`).
    pub async fn auto_apply_when_safe(self: Arc<Self>) {
        let poll_interval = if cfg!(feature = "dev") { Duration::from_secs(2) } else { AUTO_APPLY_CHECK_INTERVAL };
        let mut frontend_ready = self.frontend_ready_tx.subscribe();
        while !*frontend_ready.borrow_and_update() {
            if frontend_ready.changed().await.is_err() {
                return;
            }
        }
        let mut interval = tokio::time::interval(poll_interval);
        loop {
            interval.tick().await;
            if !cfg!(feature = "dev") && !self.settings.get().auto_update_enabled {
                continue;
            }
            if self.trigger_apply_if_safe() {
                tracing::info!("a staged update is ready and safe to apply");
            }
        }
    }
}

pub(super) fn activity_is_safe_to_apply(monitor_active: bool, recording_active: bool) -> bool {
    cfg!(feature = "dev") || (!monitor_active && !recording_active)
}
