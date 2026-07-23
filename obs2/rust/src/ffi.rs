use std::ffi::{c_char, c_void};

/// Opaque capture context owning reusable OBS render/stage surfaces. Create with
/// [`ge_capture_create`], capture via [`ge_capture_get_frame`], release with
/// [`ge_capture_destroy`]. The monitor loop holds one so GPU surfaces persist.
#[repr(C)]
pub struct GeCaptureCtx {
    _private: [u8; 0],
}

/// Optional capture transform for [`ge_capture_get_frame`]: `crop_*` are `[0,1]`
/// source fractions scaled into `out_*` (mirrors `ge_capture_region`). `GeFrameCb`
/// is a per-frame graphics-thread render callback; `cx`/`cy` = canvas dims.
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
#[allow(dead_code)]
enum ObsTaskType {
    Ui,
    Graphics,
    Audio,
    Destroy,
}

type ObsTask = unsafe extern "C" fn(param: *mut c_void);

pub(crate) fn queue_ui_task(task: ObsTask, param: *mut c_void) {
    unsafe {
        obs_queue_task(ObsTaskType::Ui, task, param, false);
    }
}

/// Severity for [`ge_obs_blog`].
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

unsafe extern "C" {
    /// Queues work onto one of OBS's task threads. UI-sensitive native dialogs
    /// should be routed through `OBS_TASK_UI`.
    fn obs_queue_task(task_type: ObsTaskType, task: ObsTask, param: *mut c_void, wait: bool);

    pub fn obs_frontend_recording_start();
    pub fn obs_frontend_recording_stop();

    /// Begins the replay buffer output (a no-op if it is not enabled in the
    /// profile, or already running). Starting is asynchronous.
    pub fn obs_frontend_replay_buffer_start();
    /// Stops the replay buffer output.
    pub fn obs_frontend_replay_buffer_stop();
    /// Writes the buffered window to disk. The save is asynchronous; OBS fires
    /// `OBS_FRONTEND_EVENT_REPLAY_BUFFER_SAVED` (handled in `core.c`, forwarded
    /// to `ge_replay_buffer_saved`) once the file is written.
    #[cfg(not(test))]
    pub fn obs_frontend_replay_buffer_save();
    /// Whether the replay buffer output is currently running.
    pub fn obs_frontend_replay_buffer_active() -> bool;

    /// Whether the replay buffer is enabled in the active profile's output
    /// settings (the "Enable Replay Buffer" checkbox). See the C bridge.
    pub fn ge_obs_replay_buffer_enabled() -> bool;
    /// Whether OBS currently has a usable replay-buffer output object. This can
    /// be false even when the profile checkbox is true, such as simple lossless
    /// recording where OBS disables replay buffer.
    pub fn ge_obs_replay_buffer_available() -> bool;
    /// Configured maximum replay-buffer duration in seconds, or -1 if the
    /// active profile config cannot be read.
    pub fn ge_obs_replay_buffer_max_seconds() -> i64;
    /// Configured directory OBS writes replay-buffer files into. Returns false
    /// when OBS cannot provide one or `buffer` is too small.
    pub fn ge_obs_replay_buffer_output_directory(buffer: *mut c_char, buffer_size: usize) -> bool;
    /// Current plugin OBS data path. Returns false when OBS cannot provide one or `buffer` is too small.
    pub fn ge_obs_module_data_path(buffer: *mut c_char, buffer_size: usize) -> bool;
    /// Configured OBS video frame rate. Falls back to active render FPS when the
    /// configured rate cannot be read. Returns 0.0 if OBS cannot provide either.
    pub fn ge_obs_video_fps() -> f64;
    pub fn ge_obs_collect_source_names(buffer: *mut c_char, buffer_size: usize);
    /// Renders the named source to a freshly `malloc`'d BGRA buffer
    /// (`width*height*4`), writing dims to out params; null if not found. Caller
    /// frees via [`free`]. Spins up a throwaway ctx; repeat callers should reuse one.
    pub fn ge_obs_get_source_frame(source_name: *const c_char, out_width: *mut u32, out_height: *mut u32) -> *mut u8;

    /// Creates a capture context; null on failure, release via [`ge_capture_destroy`].
    /// When `double_buffered`, readback is pipelined (one frame latency) and the first
    /// frame after creation/resize primes the pipeline, returning null even on success.
    pub fn ge_capture_create(double_buffered: bool) -> *mut GeCaptureCtx;
    /// Renders the source into a `malloc`'d BGRA buffer via the context's surfaces;
    /// same ownership as [`ge_obs_get_source_frame`] (free via [`free`]). `region`
    /// captures/resizes a sub-rect; else `max_height` downscales; null+0 = native.
    pub fn ge_capture_get_frame(
        ctx: *mut GeCaptureCtx,
        source_name: *const c_char,
        max_height: u32,
        region: *const GeCaptureRegion,
        out_width: *mut u32,
        out_height: *mut u32,
    ) -> *mut u8;
    /// Destroys a capture context and its surfaces.
    pub fn ge_capture_destroy(ctx: *mut GeCaptureCtx);

    /// Registers a per-frame render callback. While registered, `cb(param, ..)`
    /// fires once per rendered frame on the graphics thread (inside a graphics
    /// context), so it may capture via [`ge_capture_get_frame`] directly.
    pub fn ge_obs_register_frame_callback(cb: GeFrameCb, param: *mut c_void);
    /// Unregisters a callback registered with [`ge_obs_register_frame_callback`].
    /// Serializes with callback invocation: once it returns, `cb` is neither
    /// running nor will run again, so `param` is safe to free.
    pub fn ge_obs_unregister_frame_callback(cb: GeFrameCb, param: *mut c_void);

    /// Emits one pre-formatted line into OBS's log via `blog`. The message is
    /// passed through `blog`'s `"%s"`, so any `%` it contains is literal. The
    /// bridge maps [`GeLogLevel`] to the OBS `LOG_*` level.
    pub fn ge_obs_blog(level: GeLogLevel, msg: *const c_char);

    /// libc `free`, used to release buffers handed back by the C bridge.
    pub fn free(ptr: *mut c_void);

    /// Wakes the shim's reload worker to apply a staged update (see reload.h).
    /// Safe from any context, but never from a tokio worker whose runtime the
    /// reload tears down; see update_apply.rs's `trigger_apply` (detached thread).
    pub fn ge_core_trigger_reload();
}
