use super::EnvVar;

static GE_CV_BENCH: EnvVar = EnvVar::new("GE_CV_BENCH");
static GE_CV_BENCH_CAPTURE: EnvVar = EnvVar::new("GE_CV_BENCH_CAPTURE");
static GE_CV_BENCH_JSON: EnvVar = EnvVar::new("GE_CV_BENCH_JSON");
static GE_CV_BENCH_WARM: EnvVar = EnvVar::new("GE_CV_BENCH_WARM");
static GE_CV_BENCH_WARMUPS: EnvVar = EnvVar::new("GE_CV_BENCH_WARMUPS");
static GE_CV_DIAGNOSTICS: EnvVar = EnvVar::new("GE_CV_DIAGNOSTICS");
static GE_CV_THREADS: EnvVar = EnvVar::new("GE_CV_THREADS");

pub fn diagnostics_enabled() -> bool {
    GE_CV_DIAGNOSTICS.truthy()
}

pub fn threads_override() -> Option<i32> {
    GE_CV_THREADS.string().and_then(|value| value.parse().ok())
}

pub fn bench_capture_mode() -> String {
    GE_CV_BENCH_CAPTURE.string().unwrap_or_else(|| "fixture".to_owned())
}

pub fn bench_runs() -> Option<usize> {
    GE_CV_BENCH.string().map(|value| value.parse().unwrap_or(5))
}

pub fn bench_warmups() -> usize {
    GE_CV_BENCH_WARMUPS.string().and_then(|value| value.parse().ok()).unwrap_or(0)
}

pub fn bench_json() -> bool {
    GE_CV_BENCH_JSON.truthy()
}

pub fn bench_warm_path() -> Option<String> {
    GE_CV_BENCH_WARM.string()
}
