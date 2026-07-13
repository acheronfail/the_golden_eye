//! Minimal OBS C-bridge stubs to satisfy the linker for targets that never call real
//! OBS (`ge_rust` test bin, `test_match`/`annotate_match`): rustc keeps `#[no_mangle]`
//! FFI code in the rlib, so any linker must resolve it. Types opaque for standalone use.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;
use std::sync::{LazyLock, Mutex};

type ObsTask = unsafe extern "C" fn(*mut c_void);
type FrameCallback = unsafe extern "C" fn(*mut c_void, u32, u32);

static DOCK_JSON: LazyLock<Mutex<CString>> = LazyLock::new(|| Mutex::new(CString::new("[]").unwrap()));

#[unsafe(no_mangle)]
pub unsafe extern "C" fn obs_queue_task(_kind: c_int, task: ObsTask, param: *mut c_void, _wait: bool) {
    // SAFETY: stubs execute queued work synchronously.
    unsafe { task(param) };
}

#[unsafe(no_mangle)]
pub extern "C" fn obs_frontend_recording_start() {}

#[unsafe(no_mangle)]
pub extern "C" fn obs_frontend_recording_stop() {}

#[unsafe(no_mangle)]
pub extern "C" fn obs_frontend_replay_buffer_start() {}

#[unsafe(no_mangle)]
pub extern "C" fn obs_frontend_replay_buffer_stop() {}

#[unsafe(no_mangle)]
pub extern "C" fn obs_frontend_replay_buffer_save() {}

#[unsafe(no_mangle)]
pub extern "C" fn obs_frontend_replay_buffer_active() -> bool {
    false
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_replay_buffer_enabled() -> bool {
    false
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_replay_buffer_available() -> bool {
    false
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_replay_buffer_max_seconds() -> i64 {
    -1
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_replay_buffer_output_directory(_buffer: *mut c_char, _buffer_size: usize) -> bool {
    false
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_module_data_path(_buffer: *mut c_char, _buffer_size: usize) -> bool {
    false
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_module_binary_path(_buffer: *mut c_char, _buffer_size: usize) -> bool {
    false
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_video_fps() -> f64 {
    60.0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ge_obs_collect_source_names(buffer: *mut c_char, size: usize) {
    if !buffer.is_null() && size > 0 {
        // SAFETY: buffer is non-null and has at least one byte.
        unsafe { *buffer = 0 };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ge_obs_get_source_frame(
    _source: *const c_char,
    _out_width: *mut u32,
    _out_height: *mut u32,
) -> *mut u8 {
    ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_capture_create(_double_buffered: bool) -> *mut c_void {
    ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ge_capture_get_frame(
    _ctx: *mut c_void,
    _source: *const c_char,
    _max_height: u32,
    _region: *const c_void,
    _out_width: *mut u32,
    _out_height: *mut u32,
) -> *mut u8 {
    ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ge_capture_destroy(_ctx: *mut c_void) {}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_register_frame_callback(_cb: FrameCallback, _param: *mut c_void) {}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_unregister_frame_callback(_cb: FrameCallback, _param: *mut c_void) {}

#[unsafe(no_mangle)]
pub extern "C" fn obs_frontend_get_user_config() -> *mut c_void {
    ptr::dangling_mut::<u8>().cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn config_get_string(
    _config: *mut c_void,
    _section: *const c_char,
    _name: *const c_char,
) -> *const c_char {
    DOCK_JSON.lock().unwrap().as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn config_set_string(
    _config: *mut c_void,
    _section: *const c_char,
    _name: *const c_char,
    value: *const c_char,
) {
    if value.is_null() {
        return;
    }
    // SAFETY: OBS config API supplies a valid string for the duration of the call.
    let value = unsafe { CStr::from_ptr(value) };
    *DOCK_JSON.lock().unwrap() = CString::new(value.to_bytes()).unwrap();
}

#[unsafe(no_mangle)]
pub extern "C" fn config_save_safe(_config: *mut c_void, _temp: *const c_char, _backup: *const c_char) -> c_int {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_core_trigger_reload() {}

// `c_int` stands in for the ABI-identical `ffi::GeLogLevel` (a repr(C) fieldless
// enum is int-sized); the bins that include this stub can't see that crate-private type.
#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_blog(_level: c_int, _msg: *const c_char) {}
