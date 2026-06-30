use std::ffi::{c_char, c_void};

/// Opaque capture context owning the reusable OBS render/stage surfaces. Create
/// one with [`ge_capture_create`], capture frames through it with
/// [`ge_capture_get_frame`], and release it with [`ge_capture_destroy`]. A
/// repeated caller (the monitor loop) holds one of these so the GPU surfaces
/// aren't recreated per frame.
#[repr(C)]
pub struct GeCaptureCtx {
    _private: [u8; 0],
}

/// Optional capture transform handed to [`ge_capture_get_frame`] once the
/// matcher has calibrated the source's true 4:3 picture. `crop_*` are fractions
/// in `[0, 1]` of the source; only that sub-rectangle is rendered, scaled per
/// axis to fill `out_width` x `out_height`. Mirrors `struct ge_capture_region`
/// in `obs_bridge.h` -- the field layout must stay in sync.
/// Signature of an OBS per-frame render callback, as registered by
/// [`ge_obs_register_frame_callback`]. Invoked once per rendered frame on the
/// graphics thread, inside an active graphics context. `cx`/`cy` are the main
/// canvas dimensions (unused by the monitor, which captures a named source).
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

unsafe extern "C" {
    pub fn obs_frontend_recording_start();
    pub fn obs_frontend_recording_stop();
    pub fn ge_obs_collect_source_names(buffer: *mut c_char, buffer_size: usize);
    /// Renders the named source to a freshly `malloc`'d BGRA pixel buffer
    /// (`width * height * 4` bytes) and writes its dimensions to the out
    /// params. Returns null if the source can't be found or rendered. The
    /// caller owns the buffer and must release it with [`free`]. Internally
    /// spins up a throwaway [`GeCaptureCtx`]; repeated callers should hold a
    /// context and use [`ge_capture_get_frame`] instead.
    pub fn ge_obs_get_source_frame(source_name: *const c_char, out_width: *mut u32, out_height: *mut u32) -> *mut u8;

    /// Creates a capture context (allocating its reusable texrender). Returns
    /// null on failure. Release it with [`ge_capture_destroy`].
    pub fn ge_capture_create() -> *mut GeCaptureCtx;
    /// Renders the named source into a freshly `malloc`'d BGRA buffer using the
    /// context's reusable surfaces. Same ownership contract as
    /// [`ge_obs_get_source_frame`]: the caller owns the buffer and must release
    /// it with [`free`]. When `region` is non-null, only its source
    /// sub-rectangle is captured, resized to `region.out_width` x
    /// `region.out_height` (`max_height` is ignored). When `region` is null and
    /// `max_height` is non-zero and the source is taller, the frame is
    /// downscaled on the GPU to that height (preserving aspect ratio); pass null
    /// and 0 to capture the whole source at native resolution. The captured
    /// dimensions are written to the out params, and the stagesurface is
    /// recreated if they change. Returns null if the source can't be found or
    /// rendered.
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

    /// libc `free`, used to release buffers handed back by the C bridge.
    pub fn free(ptr: *mut c_void);
}
