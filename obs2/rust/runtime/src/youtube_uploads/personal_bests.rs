//! Schedules newly saved personal bests through the normal upload workflow.
use std::sync::Arc;

use ge_clip::ClipMetadata;
use tokio::runtime::Handle;
use tokio::sync::broadcast;

use super::{QueueError, UploadTrigger, YoutubeUploadStatus, YoutubeUploadStore};
use crate::app::AppEvent;
use crate::settings::SettingsStore;

#[derive(Clone)]
pub(crate) struct PersonalBestUploader {
    settings: Arc<SettingsStore>,
    uploads: YoutubeUploadStore,
    event_tx: broadcast::Sender<AppEvent>,
    runtime: Handle,
}

impl PersonalBestUploader {
    pub(crate) fn new(
        settings: Arc<SettingsStore>,
        uploads: YoutubeUploadStore,
        event_tx: broadcast::Sender<AppEvent>,
    ) -> Self {
        Self { settings, uploads, event_tx, runtime: Handle::current() }
    }

    pub(crate) fn clip_saved(&self, path: &str, metadata: &ClipMetadata) {
        let was_personal_best = metadata.was_personal_best;
        let uploader = self.clone();
        let path = path.to_owned();
        let run_id = metadata.run_id.clone();
        self.runtime.spawn(async move {
            uploader.queue_clip_with(&path, &run_id, was_personal_best, || {
                uploader.uploads.queue_upload(
                    &uploader.settings,
                    &path,
                    None,
                    UploadTrigger::PersonalBest,
                    uploader.event_tx.clone(),
                )
            });
        });
    }

    fn queue_clip_with(
        &self,
        path: &str,
        run_id: &str,
        was_personal_best: bool,
        queue: impl FnOnce() -> Result<(YoutubeUploadStatus, bool), QueueError>,
    ) {
        if !was_personal_best {
            return;
        }
        if !self.settings.get().youtube_auto_upload_personal_bests {
            tracing::debug!(%run_id, reason = "setting_disabled", "personal-best upload skipped");
            return;
        }
        match queue() {
            Ok((upload, true)) => {
                let _ = self.event_tx.send(AppEvent::YoutubePersonalBestUploadStarted { upload });
            }
            Ok((upload, false)) => {
                tracing::debug!(%run_id, upload_id = %upload.id, reason = "already_queued", "personal-best upload skipped")
            }
            Err(QueueError::Disabled) => {
                tracing::debug!(%run_id, reason = "build_disabled", "personal-best upload skipped")
            }
            Err(QueueError::Disconnected) => {
                tracing::debug!(%run_id, reason = "disconnected", "personal-best upload skipped")
            }
            Err(error) => tracing::warn!(%run_id, %path, ?error, "failed to queue personal-best upload"),
        }
    }
}

#[cfg(test)]
#[path = "personal_bests_tests.rs"]
mod tests;
