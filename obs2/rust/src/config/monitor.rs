use super::EnvVar;

static GE_MONITOR_TIMING: EnvVar = EnvVar::new("GE_MONITOR_TIMING");
static GE_MONITOR_SLOW_MS: EnvVar = EnvVar::new("GE_MONITOR_SLOW_MS");

/// Controls how much live monitor capture/match timing is logged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MonitorTimingMode {
    Off,
    Slow,
    Verbose,
}

impl MonitorTimingMode {
    /// Reads monitor timing mode from `GE_MONITOR_TIMING`.
    pub(crate) fn from_env() -> Self {
        match GE_MONITOR_TIMING.string() {
            Some(value) if matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "slow") => Self::Slow,
            Some(value) if value.eq_ignore_ascii_case("verbose") => Self::Verbose,
            _ => Self::Off,
        }
    }
}

/// Controls the slow-frame threshold for monitor timing logs.
pub(crate) fn default_monitor_slow_ms(source_fps: f64) -> f64 {
    let frame_ms = if source_fps > 0.0 { 1000.0 / source_fps } else { 16.67 };
    GE_MONITOR_SLOW_MS
        .string()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or((frame_ms * 2.0).max(40.0))
}
