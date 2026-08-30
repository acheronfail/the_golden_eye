use std::ffi::CStr;

use super::raw;
pub use super::raw::GeLogLevel;

pub fn log(level: GeLogLevel, message: &CStr) {
    unsafe { raw::ge_obs_blog(level, message.as_ptr()) };
}
