use std::env;
use std::ffi::OsString;

pub(super) fn env_truthy(name: &str) -> bool {
    env_string(name).as_deref().is_some_and(env_truthy_value)
}

/// Interprets common environment-style truthy strings consistently across config domains.
pub(crate) fn env_value_enabled(value: &str) -> bool {
    env_truthy_value(value)
}

fn env_truthy_value(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

pub(super) fn env_string(name: &str) -> Option<String> {
    env::var(name).ok()
}

pub(super) fn env_os(name: &str) -> Option<OsString> {
    env::var_os(name)
}
