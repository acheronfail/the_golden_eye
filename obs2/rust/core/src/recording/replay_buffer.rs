//! OBS-facing replay queries and callbacks backed by one coordinator.

use std::path::PathBuf;

use super::replay_coordinator::ReplayCoordinator;
#[cfg(not(test))]
pub(super) use super::replay_coordinator::{ReplaySavePermit, ReplaySaveWait};

static COORDINATOR: ReplayCoordinator = ReplayCoordinator::new();

#[cfg(not(test))]
pub(super) fn acquire_replay_save() -> ReplaySavePermit<'static> {
    COORDINATOR.acquire_save()
}

pub fn on_replay_saved(path: Option<String>) {
    COORDINATOR.on_replay_saved(path)
}

pub fn on_replay_buffer_starting() {
    COORDINATOR.on_replay_buffer_starting()
}

pub fn on_replay_buffer_started() {
    COORDINATOR.on_replay_buffer_started()
}

pub fn on_replay_buffer_stopping() {
    COORDINATOR.on_replay_buffer_stopping()
}

pub fn on_replay_buffer_stopped() {
    COORDINATOR.on_replay_buffer_stopped()
}

pub fn ensure_replay_buffer_running() -> bool {
    COORDINATOR.ensure_replay_buffer_running()
}

pub fn stop_replay_buffer_if_active() {
    COORDINATOR.stop_replay_buffer_if_active()
}

/// Whether the replay buffer is enabled in the active profile (the OBS "Enable
/// Replay Buffer" checkbox). Distinct from [`replay_buffer_active`].
pub fn replay_buffer_enabled() -> bool {
    crate::obs::replay_buffer_enabled()
}

/// Whether OBS currently exposes a replay-buffer output. This can be false even
/// when the checkbox is enabled, for output modes where OBS disables replay
/// buffer internally.
pub fn replay_buffer_available() -> bool {
    crate::obs::replay_buffer_available()
}

/// Configured maximum replay-buffer duration in seconds.
pub fn replay_buffer_max_seconds() -> Option<u64> {
    crate::obs::replay_buffer_max_seconds()
}

/// Directory OBS is configured to write replay-buffer files into.
pub fn replay_buffer_output_directory() -> Option<PathBuf> {
    crate::obs::replay_buffer_output_directory()
}

/// Whether the replay buffer output is currently running.
pub fn replay_buffer_active() -> bool {
    crate::obs::replay_buffer_active()
}

#[cfg(not(test))]
pub(super) fn ensure_replay_buffer_running_for_recording() -> bool {
    ensure_replay_buffer_running()
}

#[cfg(test)]
pub(super) fn ensure_replay_buffer_running_for_recording() -> bool {
    true
}
