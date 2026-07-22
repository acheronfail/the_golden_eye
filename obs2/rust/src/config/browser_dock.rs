use super::EnvVar;

static GE_DISABLE_BROWSER_DOCK: EnvVar = EnvVar::new("GE_DISABLE_BROWSER_DOCK");
static GE_BROWSER_DOCK_URL: EnvVar = EnvVar::new("GE_BROWSER_DOCK_URL");

/// Controls whether OBS custom browser dock setup is skipped entirely.
pub(crate) fn browser_dock_disabled() -> bool {
    GE_DISABLE_BROWSER_DOCK.truthy()
}

/// Controls the URL used when registering the OBS custom browser dock.
pub(crate) fn browser_dock_url() -> String {
    GE_BROWSER_DOCK_URL.string().filter(|value| !value.is_empty()).unwrap_or_else(|| super::loopback_http_url("/"))
}
