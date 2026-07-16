use std::time::Instant;

// Lightweight phase timer. When the GE_CV_TIMING environment variable is set,
// each lap() logs the milliseconds elapsed since the previous lap to stderr.
pub struct PhaseTimer {
    start: Instant,
    last: Instant,
    enabled: bool,
}

impl PhaseTimer {
    pub fn new() -> Self {
        let now = Instant::now();
        PhaseTimer { start: now, last: now, enabled: crate::config::cv_timing_enabled() }
    }

    pub fn start(&self) -> Instant {
        self.start
    }

    pub fn lap(&mut self, label: &str) {
        let now = Instant::now();
        if self.enabled {
            let ms = now.duration_since(self.last).as_secs_f64() * 1000.0;
            let total = now.duration_since(self.start).as_secs_f64() * 1000.0;
            eprintln!("[ge_cv timing] {label:<22} {ms:8.2} ms  (total {total:8.2} ms)");
        }
        self.last = now;
    }
}
