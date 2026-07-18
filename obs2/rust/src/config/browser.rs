use super::shared::env_truthy;

const GE_BROWSER_WS_LOG: &str = "GE_BROWSER_WS_LOG";

/// Controls browser-console logging of app WebSocket traffic.
pub(crate) fn browser_ws_log_enabled() -> bool {
    env_truthy(GE_BROWSER_WS_LOG)
}
