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
    /// it with [`free`]. When `max_height` is non-zero and the source is taller,
    /// the frame is downscaled on the GPU to that height (preserving aspect
    /// ratio); pass 0 to capture at native resolution. The captured dimensions
    /// are written to the out params, and the stagesurface is recreated if they
    /// change. Returns null if the source can't be found or rendered.
    pub fn ge_capture_get_frame(
        ctx: *mut GeCaptureCtx,
        source_name: *const c_char,
        max_height: u32,
        out_width: *mut u32,
        out_height: *mut u32,
    ) -> *mut u8;
    /// Destroys a capture context and its surfaces.
    pub fn ge_capture_destroy(ctx: *mut GeCaptureCtx);

    /// libc `free`, used to release buffers handed back by the C bridge.
    pub fn free(ptr: *mut c_void);
}
