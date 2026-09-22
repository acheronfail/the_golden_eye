use tracing_subscriber::EnvFilter;

const RUST_LOG: &str = "RUST_LOG";

/// Controls tracing filter configuration for backend logs routed into OBS.
pub(crate) fn logging_filter(crate_name: &str) -> EnvFilter {
    EnvFilter::try_from_env(RUST_LOG)
        .unwrap_or_else(|_| format!("{crate_name}={level},tower_http={level}", level = default_log_level()).into())
}

fn default_log_level() -> &'static str {
    if cfg!(debug_assertions) { "debug" } else { "info" }
}
