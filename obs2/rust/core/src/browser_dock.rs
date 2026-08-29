use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;

use serde_json::{Value, json};

const DOCK_TITLE: &str = "The Golden Eye";
const DOCK_UUID: &str = "thegoldeneyedashboard";

const DOCKS_SECTION: &[u8] = b"BasicWindow\0";
const DOCKS_KEY: &[u8] = b"ExtraBrowserDocks\0";
const CONFIG_TEMP_EXT: &[u8] = b"tmp\0";

pub fn post_load() {
    if crate::config::browser_dock_disabled() {
        tracing::info!("custom browser dock setup disabled by GE_DISABLE_BROWSER_DOCK");
        return;
    }

    let config = unsafe { obs_frontend_get_user_config() };
    if config.is_null() {
        tracing::warn!("OBS user config unavailable; could not ensure custom browser dock");
        return;
    }

    let url = crate::config::browser_dock_url();
    let existing = unsafe {
        let ptr = config_get_string(config, DOCKS_SECTION.as_ptr().cast(), DOCKS_KEY.as_ptr().cast());
        c_string(ptr)
    };

    let output = match ensure_dock_json(existing.as_deref(), DOCK_TITLE, &url, DOCK_UUID) {
        Ok(Some(output)) => output,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!("{error}; leaving OBS custom browser docks config unchanged");
            return;
        }
    };

    let output = match CString::new(output) {
        Ok(output) => output,
        Err(_) => {
            tracing::warn!("generated OBS custom browser docks config contained an interior NUL");
            return;
        }
    };

    unsafe {
        config_set_string(config, DOCKS_SECTION.as_ptr().cast(), DOCKS_KEY.as_ptr().cast(), output.as_ptr());
    }

    let save_result = unsafe { config_save_safe(config, CONFIG_TEMP_EXT.as_ptr().cast(), ptr::null()) };
    if save_result != 0 {
        tracing::warn!("could not save OBS custom browser dock config");
    } else {
        tracing::info!(%url, "ensured OBS custom browser dock");
    }
}

fn ensure_dock_json(existing: Option<&str>, title: &str, url: &str, uuid: &str) -> Result<Option<String>, String> {
    let mut docks = load_extra_browser_docks(existing)?;
    if docks.iter().any(|dock| dock_matches(dock, title, url, uuid)) {
        return Ok(None);
    }

    docks.push(json!({
        "title": title,
        "url": url,
        "uuid": uuid,
    }));

    serde_json::to_string(&docks)
        .map(Some)
        .map_err(|error| format!("could not serialize OBS custom browser docks config: {error}"))
}

fn load_extra_browser_docks(existing: Option<&str>) -> Result<Vec<Value>, String> {
    let Some(existing) = existing else {
        return Ok(Vec::new());
    };
    if existing.trim().is_empty() {
        return Ok(Vec::new());
    }

    match serde_json::from_str::<Value>(existing) {
        Ok(Value::Array(docks)) => Ok(docks),
        Ok(_) => Err("existing OBS custom browser docks config was not an array".to_string()),
        Err(error) => Err(format!("could not parse existing OBS custom browser docks config: {error}")),
    }
}

fn dock_matches(dock: &Value, title: &str, url: &str, uuid: &str) -> bool {
    string_field_eq(dock, "uuid", uuid) || string_field_eq(dock, "url", url) || string_field_eq(dock, "title", title)
}

fn string_field_eq(value: &Value, field: &str, expected: &str) -> bool {
    value.get(field).and_then(Value::as_str) == Some(expected)
}

unsafe fn c_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() { None } else { Some(unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()) }
}

unsafe extern "C" {
    fn obs_frontend_get_user_config() -> *mut c_void;
    fn config_get_string(config: *mut c_void, section: *const c_char, name: *const c_char) -> *const c_char;
    fn config_set_string(config: *mut c_void, section: *const c_char, name: *const c_char, value: *const c_char);
    fn config_save_safe(config: *mut c_void, temp_ext: *const c_char, backup_ext: *const c_char) -> c_int;
}

#[cfg(test)]
#[path = "browser_dock_test.rs"]
mod browser_dock_test;
