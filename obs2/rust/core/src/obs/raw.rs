use std::ffi::{c_char, c_int, c_void};

/// Opaque capture context owning reusable OBS render/stage surfaces.
#[repr(C)]
pub struct GeCaptureCtx {
    _private: [u8; 0],
}

pub type GeFrameCb = unsafe extern "C" fn(param: *mut c_void, cx: u32, cy: u32);

#[repr(C)]
pub struct GeCaptureRegion {
    pub crop_x: f32,
    pub crop_y: f32,
    pub crop_w: f32,
    pub crop_h: f32,
    pub out_width: u32,
    pub out_height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GeCaptureTimings {
    pub source_ms: f64,
    pub allocation_ms: f64,
    pub render_stage_ms: f64,
    pub map_copy_ms: f64,
    pub cleanup_ms: f64,
}

/// Severity passed to the C bridge's OBS logger.
///
/// cbindgen:prefix-with-name=true
#[repr(C)]
#[derive(Clone, Copy)]
pub enum GeLogLevel {
    Error = 0,
    Warning = 1,
    Info = 2,
    Debug = 3,
}

#[repr(C)]
#[allow(dead_code)]
pub(super) enum ObsTaskType {
    Ui,
    Graphics,
    Audio,
    Destroy,
}

pub(super) type ObsTask = unsafe extern "C" fn(param: *mut c_void);

unsafe extern "C" {
    pub(super) fn obs_queue_task(task_type: ObsTaskType, task: ObsTask, param: *mut c_void, wait: bool);

    pub(super) fn obs_frontend_recording_start();
    pub(super) fn obs_frontend_recording_stop();
    pub(super) fn obs_frontend_replay_buffer_start();
    pub(super) fn obs_frontend_replay_buffer_stop();
    #[cfg(not(test))]
    pub(super) fn obs_frontend_replay_buffer_save();
    pub(super) fn obs_frontend_replay_buffer_active() -> bool;

    pub(super) fn ge_obs_replay_buffer_enabled() -> bool;
    pub(super) fn ge_obs_replay_buffer_available() -> bool;
    pub(super) fn ge_obs_replay_buffer_max_seconds() -> i64;
    pub(super) fn ge_obs_replay_buffer_output_directory(buffer: *mut c_char, buffer_size: usize) -> bool;
    pub(super) fn ge_obs_module_data_path(buffer: *mut c_char, buffer_size: usize) -> bool;
    pub(super) fn ge_obs_video_fps() -> f64;
    pub(super) fn ge_obs_collect_source_names(buffer: *mut c_char, buffer_size: usize);
    /// Returns a malloc-owned BGRA frame; callers release it with [`free`].
    pub(super) fn ge_obs_get_source_frame(
        source_name: *const c_char,
        out_width: *mut u32,
        out_height: *mut u32,
    ) -> *mut u8;

    /// Creates reusable capture surfaces; release them with [`ge_capture_destroy`].
    pub(super) fn ge_capture_create(double_buffered: bool) -> *mut GeCaptureCtx;
    /// Returns a malloc-owned BGRA frame; callers release it with [`free`].
    pub(super) fn ge_capture_get_frame(
        ctx: *mut GeCaptureCtx,
        source_name: *const c_char,
        max_height: u32,
        region: *const GeCaptureRegion,
        out_width: *mut u32,
        out_height: *mut u32,
        timings: *mut GeCaptureTimings,
    ) -> *mut u8;
    pub(super) fn ge_capture_destroy(ctx: *mut GeCaptureCtx);
    pub(super) fn ge_obs_register_frame_callback(cb: GeFrameCb, param: *mut c_void);
    /// Fences callback invocation before returning, after which `param` may be freed.
    pub(super) fn ge_obs_unregister_frame_callback(cb: GeFrameCb, param: *mut c_void);

    pub(super) fn ge_obs_blog(level: GeLogLevel, msg: *const c_char);
    pub(super) fn obs_frontend_get_user_config() -> *mut c_void;
    pub(super) fn config_get_string(config: *mut c_void, section: *const c_char, name: *const c_char) -> *const c_char;
    pub(super) fn config_set_string(
        config: *mut c_void,
        section: *const c_char,
        name: *const c_char,
        value: *const c_char,
    );
    pub(super) fn config_save_safe(config: *mut c_void, temp_ext: *const c_char, backup_ext: *const c_char) -> c_int;

    /// Releases buffers allocated by the C bridge.
    pub(super) fn free(ptr: *mut c_void);
    /// Wakes the shim reload worker; never call from the tokio runtime it tears down.
    pub(super) fn ge_core_trigger_reload();
}
