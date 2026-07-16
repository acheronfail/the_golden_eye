use super::shared::{env_string, env_truthy};

const GE_DISABLE_BROWSER_DOCK: &str = "GE_DISABLE_BROWSER_DOCK";
const GE_BROWSER_DOCK_URL: &str = "GE_BROWSER_DOCK_URL";

/// Default URL registered in OBS's custom browser dock configuration.
pub(crate) const DEFAULT_BROWSER_DOCK_URL: &str = "http://127.0.0.1:31337/";

/// Controls whether OBS custom browser dock setup is skipped entirely.
pub(crate) fn browser_dock_disabled() -> bool {
    env_truthy(GE_DISABLE_BROWSER_DOCK)
}

/// Controls the URL used when registering the OBS custom browser dock.
pub(crate) fn browser_dock_url() -> String {
    env_string(GE_BROWSER_DOCK_URL)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_BROWSER_DOCK_URL.to_owned())
}
