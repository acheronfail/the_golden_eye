use std::ffi::{CStr, c_char};
use std::path::PathBuf;

use super::raw;

pub fn start_recording() {
    unsafe { raw::obs_frontend_recording_start() };
}

pub fn stop_recording() {
    unsafe { raw::obs_frontend_recording_stop() };
}

pub fn start_replay_buffer() {
    unsafe { raw::obs_frontend_replay_buffer_start() };
}

pub fn stop_replay_buffer() {
    unsafe { raw::obs_frontend_replay_buffer_stop() };
}

#[cfg(not(test))]
pub fn save_replay_buffer() {
    unsafe { raw::obs_frontend_replay_buffer_save() };
}

pub fn replay_buffer_active() -> bool {
    unsafe { raw::obs_frontend_replay_buffer_active() }
}

pub fn replay_buffer_enabled() -> bool {
    unsafe { raw::ge_obs_replay_buffer_enabled() }
}

pub fn replay_buffer_available() -> bool {
    unsafe { raw::ge_obs_replay_buffer_available() }
}

pub fn replay_buffer_max_seconds() -> Option<u64> {
    u64::try_from(unsafe { raw::ge_obs_replay_buffer_max_seconds() }).ok()
}

fn read_path(getter: unsafe extern "C" fn(*mut c_char, usize) -> bool) -> Option<PathBuf> {
    let mut buffer = vec![0 as c_char; 4096];
    if !unsafe { getter(buffer.as_mut_ptr(), buffer.len()) } {
        return None;
    }
    let path = unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy().trim().to_owned();
    if path.is_empty() { None } else { Some(PathBuf::from(path)) }
}

pub fn replay_buffer_output_directory() -> Option<PathBuf> {
    read_path(raw::ge_obs_replay_buffer_output_directory)
}

pub fn module_data_path() -> Option<PathBuf> {
    read_path(raw::ge_obs_module_data_path)
}

pub fn video_fps() -> f64 {
    unsafe { raw::ge_obs_video_fps() }
}

pub fn source_names() -> Vec<(String, String)> {
    let mut buffer = [0 as c_char; 4096];
    unsafe { raw::ge_obs_collect_source_names(buffer.as_mut_ptr(), buffer.len()) };
    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_str()
        .unwrap_or_default()
        .split('\n')
        .filter_map(|line| {
            let (name, id) = line.split_once('\t')?;
            (!name.is_empty()).then(|| (name.to_owned(), id.to_owned()))
        })
        .collect()
}

pub fn trigger_core_reload() {
    unsafe { raw::ge_core_trigger_reload() };
}
