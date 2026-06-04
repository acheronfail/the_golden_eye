use std::ffi::{c_char, c_void};

unsafe extern "C" {
    pub fn obs_frontend_recording_start();
    pub fn obs_frontend_recording_stop();
    pub fn ge_obs_collect_source_names(buffer: *mut c_char, buffer_size: usize);
    /// Renders the named source to a freshly `malloc`'d BGRA pixel buffer
    /// (`width * height * 4` bytes) and writes its dimensions to the out
    /// params. Returns null if the source can't be found or rendered. The
    /// caller owns the buffer and must release it with [`free`].
    pub fn ge_obs_get_source_frame(
        source_name: *const c_char,
        out_width: *mut u32,
        out_height: *mut u32,
    ) -> *mut u8;

    /// libc `free`, used to release buffers handed back by the C bridge.
    pub fn free(ptr: *mut c_void);
}
