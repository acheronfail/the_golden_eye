//! Watches GoldenEye gameplay, recognizes runs, estimates game time, and saves clips.
//! Start with `session.rs` for the workflow and `run_detection.rs` for run transitions.

mod clip_output;
mod clip_saving;
mod clocks;
pub(crate) mod in_game_timer;
mod lifecycle;
mod matcher;
pub(crate) mod publication;
mod recording;
mod recording_status;
pub(crate) mod replay_buffer;
mod run_detection;
mod session;
mod throughput;
mod timing;
mod worker;

pub use ge_settings::{
    DEFAULT_CLIP_FILENAME_TEMPLATE,
    DEFAULT_POST_RUN_PADDING_SECS,
    DEFAULT_PRE_RUN_PADDING_SECS,
    DEFAULT_RECENT_RUN_LIMIT,
    MAX_RECENT_RUN_LIMIT,
};
pub(crate) use lifecycle::{RunMonitor, StartError, StopReason};
pub use recording::{RecordingOptions, RecordingSessionContext, RunRecorder};
pub(crate) use recording_status::RecordingStateEvent;
pub use recording_status::{RecordingStateStore, RecordingStatus};

pub use crate::obs::replay_buffer_output_directory;

const DEFAULT_MONITOR_LANGUAGE: &str = "jp";
const MATCH_PADDING_BUFFER_SECS: f64 = 0.5;

#[cfg(test)]
#[path = "tests/support.rs"]
pub(crate) mod test_support;
