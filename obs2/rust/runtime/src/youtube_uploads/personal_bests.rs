//! Schedules newly saved personal bests through the normal upload workflow.
use std::sync::Arc;

use ge_clip::ClipMetadata;
use tokio::runtime::Handle;
use tokio::sync::broadcast;

use super::{QueueError, YoutubeUploadStore};
use crate::app::AppEvent;
use crate::settings::SettingsStore;

#[cfg_attr(test, allow(dead_code))]
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

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn clip_saved(&self, path: &str, metadata: &ClipMetadata) {
        if !metadata.was_personal_best {
            return;
        }
        let uploader = self.clone();
        let path = path.to_owned();
        self.runtime.spawn(async move {
            if !uploader.settings.get().youtube_auto_upload_personal_bests {
                return;
            }
            match uploader.uploads.queue_upload(&uploader.settings, &path, None, uploader.event_tx.clone()) {
                Ok((upload, true)) => {
                    let _ = uploader.event_tx.send(AppEvent::YoutubePersonalBestUploadStarted { upload });
                }
                Ok((_, false)) | Err(QueueError::Disabled | QueueError::Disconnected) => {}
                Err(error) => tracing::warn!(%path, ?error, "failed to queue personal-best upload"),
            }
        });
    }
}
