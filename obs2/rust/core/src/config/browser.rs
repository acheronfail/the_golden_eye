use super::EnvVar;

static GE_BROWSER_WS_LOG: EnvVar = EnvVar::new("GE_BROWSER_WS_LOG");

/// Controls browser-console logging of app WebSocket traffic.
pub(crate) fn browser_ws_log_enabled() -> bool {
    GE_BROWSER_WS_LOG.truthy()
}
