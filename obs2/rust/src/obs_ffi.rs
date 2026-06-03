use std::ffi::c_char;

unsafe extern "C" {
    pub fn obs_frontend_recording_start();
    pub fn obs_frontend_recording_stop();
    pub fn ge_obs_collect_source_names(buffer: *mut c_char, buffer_size: usize);
}
