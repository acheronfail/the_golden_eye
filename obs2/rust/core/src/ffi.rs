use std::ffi::{CStr, c_char, c_int, c_void};
use std::path::PathBuf;
use std::ptr::NonNull;

/// One BGRA source frame allocated by the C bridge.
pub struct OwnedBgraFrame {
    ptr: NonNull<u8>,
    width: u32,
    height: u32,
    len: usize,
}

impl OwnedBgraFrame {
    fn from_raw(ptr: *mut u8, width: u32, height: u32) -> Option<Self> {
        let ptr = NonNull::new(ptr)?;
        let len = (width as usize).checked_mul(height as usize).and_then(|pixels| pixels.checked_mul(4));
        let Some(len) = len.filter(|len| *len > 0) else {
            // SAFETY: the bridge returned a non-null malloc allocation, but invalid
            // dimensions make it unusable as a BGRA frame.
            unsafe { free(ptr.as_ptr().cast()) };
            return None;
        };
        Some(Self { ptr, width, height, len })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn bytes(&self) -> &[u8] {
        // SAFETY: the C bridge allocated `len` bytes and this value owns them
        // until Drop returns the allocation to the C allocator.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

// SAFETY: the frame exclusively owns its allocation and only exposes immutable
// bytes, so ownership may move between the OBS and monitor threads.
unsafe impl Send for OwnedBgraFrame {}

impl Drop for OwnedBgraFrame {
    fn drop(&mut self) {
        // SAFETY: this allocation came from the C bridge's malloc and is owned
        // exclusively by this value.
        unsafe { free(self.ptr.as_ptr().cast()) };
    }
}

/// Render one OBS source into an owned BGRA frame.
pub fn capture_source_frame(source_name: &CStr) -> Option<OwnedBgraFrame> {
    let mut width = 0;
    let mut height = 0;
    let ptr = unsafe { ge_obs_get_source_frame(source_name.as_ptr(), &mut width, &mut height) };
    OwnedBgraFrame::from_raw(ptr, width, height)
}

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
#[derive(Clone, Copy, Debug, Default)]
pub struct GeCaptureTimings {
    pub source_ms: f64,
    pub allocation_ms: f64,
    pub render_stage_ms: f64,
    pub map_copy_ms: f64,
    pub cleanup_ms: f64,
}

/// Reusable OBS render/stage surfaces for repeated source capture.
pub struct CaptureContext {
    ptr: NonNull<GeCaptureCtx>,
}

// SAFETY: the context is exclusively owned and OBS callback registration
// serializes its use with teardown before it can move to another thread.
unsafe impl Send for CaptureContext {}

impl CaptureContext {
    pub fn new(double_buffered: bool) -> Option<Self> {
        NonNull::new(unsafe { ge_capture_create(double_buffered) }).map(|ptr| Self { ptr })
    }

    pub fn capture(
        &mut self,
        source_name: &CStr,
        max_height: u32,
        region: Option<&GeCaptureRegion>,
        timings: Option<&mut GeCaptureTimings>,
    ) -> Option<OwnedBgraFrame> {
        let mut width = 0;
        let mut height = 0;
        let region = region.map_or(std::ptr::null(), |region| region as *const _);
        let timings = timings.map_or(std::ptr::null_mut(), |timings| timings as *mut _);
        let ptr = unsafe {
            ge_capture_get_frame(
                self.ptr.as_ptr(),
                source_name.as_ptr(),
                max_height,
                region,
                &mut width,
                &mut height,
                timings,
            )
        };
        OwnedBgraFrame::from_raw(ptr, width, height)
    }
}

impl Drop for CaptureContext {
    fn drop(&mut self) {
        unsafe { ge_capture_destroy(self.ptr.as_ptr()) };
    }
}

/// Safe Rust callback invoked by OBS's serialized graphics-thread callback.
pub trait RenderCallback: Send + 'static {
    fn render_frame(&mut self, canvas_width: u32, canvas_height: u32);
}

/// Owns registered callback state and reclaims it only after OBS unregisters.
pub struct RegisteredRenderCallback<T: RenderCallback> {
    state: NonNull<T>,
}

// SAFETY: moving the registration does not access its state. OBS serializes the
// callback with unregister, and T is reclaimed only after unregister returns.
unsafe impl<T: RenderCallback> Send for RegisteredRenderCallback<T> {}

impl<T: RenderCallback> RegisteredRenderCallback<T> {
    pub fn register(state: T) -> Self {
        let state = NonNull::from(Box::leak(Box::new(state)));
        unsafe { ge_obs_register_frame_callback(render_callback::<T>, state.as_ptr().cast()) };
        Self { state }
    }
}

impl<T: RenderCallback> Drop for RegisteredRenderCallback<T> {
    fn drop(&mut self) {
        unsafe {
            ge_obs_unregister_frame_callback(render_callback::<T>, self.state.as_ptr().cast());
            drop(Box::from_raw(self.state.as_ptr()));
        }
    }
}

unsafe extern "C" fn render_callback<T: RenderCallback>(param: *mut c_void, cx: u32, cy: u32) {
    // SAFETY: register supplies a live Box<T>; unregister fences callbacks before
    // reclaiming it, and OBS serializes invocations on the graphics thread.
    let state = unsafe { &mut *param.cast::<T>() };
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.render_frame(cx, cy))).is_err() {
        tracing::error!("OBS render callback panicked");
    }
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
type UiTask = Box<dyn FnOnce() + Send>;

pub fn queue_ui_task(task: impl FnOnce() + Send + 'static) {
    let task: Box<UiTask> = Box::new(Box::new(task));
    unsafe { obs_queue_task(ObsTaskType::Ui, run_ui_task, Box::into_raw(task).cast(), false) };
}

unsafe extern "C" fn run_ui_task(param: *mut c_void) {
    // SAFETY: queue_ui_task passes ownership of a Box<UiTask> to this callback,
    // which OBS invokes exactly once.
    let task = unsafe { Box::from_raw(param.cast::<UiTask>()) };
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(*task)).is_err() {
        tracing::error!("OBS UI task panicked");
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

pub fn log(level: GeLogLevel, message: &CStr) {
    unsafe { ge_obs_blog(level, message.as_ptr()) };
}

pub fn start_recording() {
    unsafe { obs_frontend_recording_start() };
}

pub fn stop_recording() {
    unsafe { obs_frontend_recording_stop() };
}

pub fn start_replay_buffer() {
    unsafe { obs_frontend_replay_buffer_start() };
}

pub fn stop_replay_buffer() {
    unsafe { obs_frontend_replay_buffer_stop() };
}

#[cfg(not(test))]
pub fn save_replay_buffer() {
    unsafe { obs_frontend_replay_buffer_save() };
}

pub fn replay_buffer_active() -> bool {
    unsafe { obs_frontend_replay_buffer_active() }
}

pub fn replay_buffer_enabled() -> bool {
    unsafe { ge_obs_replay_buffer_enabled() }
}

pub fn replay_buffer_available() -> bool {
    unsafe { ge_obs_replay_buffer_available() }
}

pub fn replay_buffer_max_seconds() -> Option<u64> {
    u64::try_from(unsafe { ge_obs_replay_buffer_max_seconds() }).ok()
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
    read_path(ge_obs_replay_buffer_output_directory)
}

pub fn module_data_path() -> Option<PathBuf> {
    read_path(ge_obs_module_data_path)
}

pub fn video_fps() -> f64 {
    unsafe { ge_obs_video_fps() }
}

pub fn source_names() -> Vec<(String, String)> {
    let mut buffer = [0 as c_char; 4096];
    unsafe { ge_obs_collect_source_names(buffer.as_mut_ptr(), buffer.len()) };
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

pub fn frontend_config_string(section: &CStr, name: &CStr) -> Option<String> {
    let config = NonNull::new(unsafe { obs_frontend_get_user_config() })?;
    let value = unsafe { config_get_string(config.as_ptr(), section.as_ptr(), name.as_ptr()) };
    NonNull::new(value.cast_mut()).map(|value| unsafe { CStr::from_ptr(value.as_ptr()) }.to_string_lossy().into_owned())
}

pub fn set_frontend_config_string(section: &CStr, name: &CStr, value: &CStr, temp_ext: &CStr) -> bool {
    let Some(config) = NonNull::new(unsafe { obs_frontend_get_user_config() }) else {
        return false;
    };
    unsafe {
        config_set_string(config.as_ptr(), section.as_ptr(), name.as_ptr(), value.as_ptr());
        config_save_safe(config.as_ptr(), temp_ext.as_ptr(), std::ptr::null()) == 0
    }
}

pub fn trigger_core_reload() {
    unsafe { ge_core_trigger_reload() };
}

unsafe extern "C" {
    /// Queues work onto one of OBS's task threads. UI-sensitive native dialogs
    /// should be routed through `OBS_TASK_UI`.
    fn obs_queue_task(task_type: ObsTaskType, task: ObsTask, param: *mut c_void, wait: bool);

    fn obs_frontend_recording_start();
    fn obs_frontend_recording_stop();

    /// Begins the replay buffer output (a no-op if it is not enabled in the
    /// profile, or already running). Starting is asynchronous.
    fn obs_frontend_replay_buffer_start();
    /// Stops the replay buffer output.
    fn obs_frontend_replay_buffer_stop();
    /// Writes the buffered window to disk. The save is asynchronous; OBS fires
    /// `OBS_FRONTEND_EVENT_REPLAY_BUFFER_SAVED` (handled in `core.c`, forwarded
    /// to `ge_replay_buffer_saved`) once the file is written.
    #[cfg(not(test))]
    fn obs_frontend_replay_buffer_save();
    /// Whether the replay buffer output is currently running.
    fn obs_frontend_replay_buffer_active() -> bool;

    /// Whether the replay buffer is enabled in the active profile's output
    /// settings (the "Enable Replay Buffer" checkbox). See the C bridge.
    fn ge_obs_replay_buffer_enabled() -> bool;
    /// Whether OBS currently has a usable replay-buffer output object. This can
    /// be false even when the profile checkbox is true, such as simple lossless
    /// recording where OBS disables replay buffer.
    fn ge_obs_replay_buffer_available() -> bool;
    /// Configured maximum replay-buffer duration in seconds, or -1 if the
    /// active profile config cannot be read.
    fn ge_obs_replay_buffer_max_seconds() -> i64;
    /// Configured directory OBS writes replay-buffer files into. Returns false
    /// when OBS cannot provide one or `buffer` is too small.
    fn ge_obs_replay_buffer_output_directory(buffer: *mut c_char, buffer_size: usize) -> bool;
    /// Current plugin OBS data path. Returns false when OBS cannot provide one or `buffer` is too small.
    fn ge_obs_module_data_path(buffer: *mut c_char, buffer_size: usize) -> bool;
    /// Configured OBS video frame rate. Falls back to active render FPS when the
    /// configured rate cannot be read. Returns 0.0 if OBS cannot provide either.
    fn ge_obs_video_fps() -> f64;
    fn ge_obs_collect_source_names(buffer: *mut c_char, buffer_size: usize);
    /// Renders the named source to a freshly `malloc`'d BGRA buffer
    /// (`width*height*4`), writing dims to out params; null if not found. Caller
    /// frees via [`free`]. Spins up a throwaway ctx; repeat callers should reuse one.
    fn ge_obs_get_source_frame(source_name: *const c_char, out_width: *mut u32, out_height: *mut u32) -> *mut u8;

    /// Creates a capture context; null on failure, release via [`ge_capture_destroy`].
    /// When `double_buffered`, readback is pipelined (one frame latency) and the first
    /// frame after creation/resize primes the pipeline, returning null even on success.
    fn ge_capture_create(double_buffered: bool) -> *mut GeCaptureCtx;
    /// Renders the source into a `malloc`'d BGRA buffer via the context's surfaces;
    /// same ownership as [`ge_obs_get_source_frame`] (free via [`free`]). `region`
    /// captures/resizes a sub-rect; else `max_height` downscales; null+0 = native.
    fn ge_capture_get_frame(
        ctx: *mut GeCaptureCtx,
        source_name: *const c_char,
        max_height: u32,
        region: *const GeCaptureRegion,
        out_width: *mut u32,
        out_height: *mut u32,
        timings: *mut GeCaptureTimings,
    ) -> *mut u8;
    /// Destroys a capture context and its surfaces.
    fn ge_capture_destroy(ctx: *mut GeCaptureCtx);

    /// Registers a per-frame render callback. While registered, `cb(param, ..)`
    /// fires once per rendered frame on the graphics thread (inside a graphics
    /// context), so it may capture via [`ge_capture_get_frame`] directly.
    fn ge_obs_register_frame_callback(cb: GeFrameCb, param: *mut c_void);
    /// Unregisters a callback registered with [`ge_obs_register_frame_callback`].
    /// Serializes with callback invocation: once it returns, `cb` is neither
    /// running nor will run again, so `param` is safe to free.
    fn ge_obs_unregister_frame_callback(cb: GeFrameCb, param: *mut c_void);

    /// Emits one pre-formatted line into OBS's log via `blog`. The message is
    /// passed through `blog`'s `"%s"`, so any `%` it contains is literal. The
    /// bridge maps [`GeLogLevel`] to the OBS `LOG_*` level.
    fn ge_obs_blog(level: GeLogLevel, msg: *const c_char);

    fn obs_frontend_get_user_config() -> *mut c_void;
    fn config_get_string(config: *mut c_void, section: *const c_char, name: *const c_char) -> *const c_char;
    fn config_set_string(config: *mut c_void, section: *const c_char, name: *const c_char, value: *const c_char);
    fn config_save_safe(config: *mut c_void, temp_ext: *const c_char, backup_ext: *const c_char) -> c_int;

    /// libc `free`, used to release buffers handed back by the C bridge.
    fn free(ptr: *mut c_void);

    /// Wakes the shim's reload worker to apply a staged update (see reload.h).
    /// Safe from any context, but never from a tokio worker whose runtime the
    /// reload tears down; see update_apply.rs's `trigger_apply` (detached thread).
    fn ge_core_trigger_reload();
}
