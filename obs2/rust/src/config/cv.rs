use super::shared::env_os;

const GE_CV_DEBUG: &str = "GE_CV_DEBUG";
const GE_CV_THREADS: &str = "GE_CV_THREADS";
const GE_CV_TIMING: &str = "GE_CV_TIMING";

/// Controls verbose matcher debug output for template scores and detections.
pub(crate) fn cv_debug_enabled() -> bool {
    env_os(GE_CV_DEBUG).is_some()
}

/// Controls per-phase matcher timing output to stderr.
pub(crate) fn cv_timing_enabled() -> bool {
    env_os(GE_CV_TIMING).is_some()
}

/// Controls whether OpenCV's internal thread count is left untouched for benchmarking.
pub(crate) fn cv_threads_overridden() -> bool {
    env_os(GE_CV_THREADS).is_some()
}
