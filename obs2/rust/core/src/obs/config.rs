use std::ffi::CStr;
use std::ptr::NonNull;

use super::raw;

pub fn frontend_config_string(section: &CStr, name: &CStr) -> Option<String> {
    let config = NonNull::new(unsafe { raw::obs_frontend_get_user_config() })?;
    let value = unsafe { raw::config_get_string(config.as_ptr(), section.as_ptr(), name.as_ptr()) };
    NonNull::new(value.cast_mut()).map(|value| unsafe { CStr::from_ptr(value.as_ptr()) }.to_string_lossy().into_owned())
}

pub fn set_frontend_config_string(section: &CStr, name: &CStr, value: &CStr, temp_ext: &CStr) -> bool {
    let Some(config) = NonNull::new(unsafe { raw::obs_frontend_get_user_config() }) else {
        return false;
    };
    unsafe {
        raw::config_set_string(config.as_ptr(), section.as_ptr(), name.as_ptr(), value.as_ptr());
        raw::config_save_safe(config.as_ptr(), temp_ext.as_ptr(), std::ptr::null()) == 0
    }
}
