//! Safe Rust interface to OBS and the C core bridge.
//!
//! Application code uses this module; raw ABI declarations stay private in
//! [`raw`], so pointer and ownership contracts cannot leak into callers.

mod capture;
mod config;
mod frontend;
mod logging;
mod raw;
mod task;

pub use capture::{CaptureContext, OwnedBgraFrame, RegisteredRenderCallback, RenderCallback, capture_source_frame};
pub use config::{frontend_config_string, set_frontend_config_string};
#[cfg(not(test))]
pub use frontend::save_replay_buffer;
pub use frontend::{
    module_data_path,
    replay_buffer_active,
    replay_buffer_available,
    replay_buffer_enabled,
    replay_buffer_max_seconds,
    replay_buffer_output_directory,
    source_names,
    start_recording,
    start_replay_buffer,
    stop_recording,
    stop_replay_buffer,
    trigger_core_reload,
    video_fps,
};
pub use logging::{GeLogLevel, log};
pub use raw::{GeCaptureRegion, GeCaptureTimings};
pub use task::queue_ui_task;
