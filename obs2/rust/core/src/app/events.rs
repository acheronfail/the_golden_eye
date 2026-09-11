use serde::Serialize;

use super::AppSnapshot;
use crate::run_monitoring::publication::*;

/// Messages pushed to app event-stream clients, internally tagged by `type`.
/// Retained state is carried by `Snapshot`; the other variants are one-off
/// events sent only to connected clients.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum AppEvent {
    /// Sent once on connect: the build id of the SPA this backend serves. The
    /// SPA compares it against its own served build and reloads on mismatch, so
    /// a stale tab picks up the new frontend. See [`crate::http::routes::index::BUILD_ID`].
    Version {
        #[serde(rename = "buildId")]
        build_id: String,
    },
    /// The complete retained app/session state. Sent on connect and after every
    /// retained-state change so new tabs sync to the backend source of truth.
    Snapshot { state: Box<AppSnapshot> },
    /// Rolling monitor throughput and backend-owned matcher health. Captured
    /// frames are either processed or superseded in the latest-frame mailbox.
    MonitorFps(MonitorFps),
    /// A run's clip save was scheduled and will fire after the post-run padding.
    RecordingSavePending(RecordingSavePending),
    /// A run's clip was saved out of the replay buffer and trimmed.
    RecordingSaved(RecordingSaved),
    RunCatalogChanged {
        #[serde(rename = "runId", skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        run_id: Option<String>,
        #[serde(rename = "saveId", skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "number")]
        save_id: Option<u64>,
    },
    /// Monitoring stopped, either from a user request or an external OBS event.
    MonitorStopped { reason: MonitorStoppedReason },
    /// The settings JSON file changed on disk and was reloaded successfully.
    SettingsReloaded {
        #[serde(rename = "configPath")]
        config_path: String,
        #[ts(type = "AppSettings")]
        settings: crate::settings::AppSettings,
    },
    /// The settings JSON file changed on disk but could not be parsed or read.
    SettingsInvalid {
        #[serde(rename = "configPath")]
        config_path: String,
        error: String,
    },
    /// Sent once when a client connects shortly after this core was loaded via
    /// an applied update (dev hot-reload or a real auto-update), so the SPA
    /// can show a one-off "plugin updated" notice. See `AppStateInner::reloaded_at`.
    UpdateApplied {
        version: String,
        /// GitHub release page for `version`, but only when the persisted
        /// `last_known_update_version` matches the running version (i.e. this is
        /// the update just applied). `None` otherwise, to avoid a wrong link.
        #[serde(rename = "releaseUrl", skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        release_url: Option<String>,
    },
    /// A newer release was found but downloading/verifying/staging it failed
    /// (e.g. an unwritable install directory), so no update is queued up to
    /// apply. One-off, delivered via `event_tx` -- see `updates::check_for_updates_now`.
    UpdateStagingFailed { error: String },
    /// A YouTube upload was queued, progressed, completed, or failed.
    YoutubeUploadChanged { upload: crate::youtube_uploads::YoutubeUploadStatus },
    /// YouTube connection state changed in another browser client.
    YoutubeStatusChanged { status: crate::youtube_uploads::YoutubeStatus },
}
