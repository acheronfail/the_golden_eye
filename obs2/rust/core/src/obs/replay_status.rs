use crate::settings::default_completed_output_path;

/// Replay-buffer status: `enabled` (OBS profile checkbox), `available` (OBS has
/// an output object for current settings), `active` (currently running).
/// Mirrored by the frontend's `ReplayBufferStatus`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ReplayBufferStatus {
    pub enabled: bool,
    pub available: bool,
    pub active: bool,
    #[ts(type = "number | null")]
    pub max_seconds: Option<u64>,
    pub output_directory: Option<String>,
    pub default_completed_output_path: Option<String>,
}

impl ReplayBufferStatus {
    pub fn unknown() -> Self {
        Self {
            enabled: false,
            available: false,
            active: false,
            max_seconds: None,
            output_directory: None,
            default_completed_output_path: None,
        }
    }
}

/// Reports whether the replay buffer is enabled/available in OBS (and running),
/// so the frontend can prompt the user before starting a session.
pub fn current_replay_buffer_status() -> ReplayBufferStatus {
    let output_directory = crate::obs::replay_buffer_output_directory();
    let default_completed_output_path =
        output_directory.as_deref().map(default_completed_output_path).map(|path| path.to_string_lossy().into_owned());
    ReplayBufferStatus {
        enabled: crate::obs::replay_buffer_enabled(),
        available: crate::obs::replay_buffer_available(),
        active: crate::obs::replay_buffer_active(),
        max_seconds: crate::obs::replay_buffer_max_seconds(),
        output_directory: output_directory.map(|path| path.to_string_lossy().into_owned()),
        default_completed_output_path,
    }
}
