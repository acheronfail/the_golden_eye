use super::EnvVar;

static GE_CV_DEBUG: EnvVar = EnvVar::new("GE_CV_DEBUG");
static GE_CV_THREADS: EnvVar = EnvVar::new("GE_CV_THREADS");
static GE_CV_TIMING: EnvVar = EnvVar::new("GE_CV_TIMING");

/// Controls verbose matcher debug output for template scores and detections.
pub(crate) fn cv_debug_enabled() -> bool {
    GE_CV_DEBUG.is_set()
}

/// Controls per-phase matcher timing output to stderr.
pub(crate) fn cv_timing_enabled() -> bool {
    GE_CV_TIMING.is_set()
}

/// Controls whether OpenCV's internal thread count is left untouched for benchmarking.
pub(crate) fn cv_threads_overridden() -> bool {
    GE_CV_THREADS.is_set()
}
