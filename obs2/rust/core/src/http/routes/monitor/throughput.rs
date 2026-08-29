use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::http::{MonitorFps, MonitorFpsHealth};

const WINDOW: Duration = Duration::from_millis(750);
const WARMUP: Duration = Duration::from_millis(250);
const EMIT_INTERVAL: Duration = Duration::from_millis(100);
const LAG_DURATION: Duration = Duration::from_millis(500);
const RECOVERY_DURATION: Duration = Duration::from_secs(1);
const MIN_PROCESSING_RATIO: f64 = 0.95;

#[derive(Clone, Copy)]
struct Sample {
    at: Instant,
    processed: u64,
    captured: u64,
    dropped: u64,
}

/// Rolling producer/consumer throughput. A captured frame is either processed
/// or superseded in the capacity-one mailbox, so captured = processed + dropped.
pub(super) struct ThroughputMeter {
    samples: VecDeque<Sample>,
    processed: u64,
    source_fps: f64,
    last_emit: Instant,
    low_since: Option<Instant>,
    healthy_since: Option<Instant>,
    health: MonitorFpsHealth,
}

impl ThroughputMeter {
    pub(super) fn new(now: Instant, source_fps: f64) -> Self {
        let samples = VecDeque::from([Sample { at: now, processed: 0, captured: 0, dropped: 0 }]);
        Self {
            samples,
            processed: 0,
            source_fps,
            last_emit: now,
            low_since: None,
            healthy_since: None,
            health: MonitorFpsHealth::Healthy,
        }
    }

    pub(super) fn observe(&mut self, now: Instant, dropped: u64) -> Option<MonitorFps> {
        self.processed += 1;
        self.samples.push_back(Sample {
            at: now,
            processed: self.processed,
            captured: self.processed + dropped,
            dropped,
        });
        let cutoff = now.checked_sub(WINDOW).unwrap_or(now);
        while self.samples.len() > 2 && self.samples[1].at <= cutoff {
            self.samples.pop_front();
        }

        if now.duration_since(self.last_emit) < EMIT_INTERVAL {
            return None;
        }
        self.last_emit = now;

        let first = *self.samples.front()?;
        let last = *self.samples.back()?;
        let elapsed = last.at.duration_since(first.at);
        if elapsed < WARMUP {
            return None;
        }

        let seconds = elapsed.as_secs_f64();
        let processed_fps = (last.processed - first.processed) as f64 / seconds;
        let captured_fps = (last.captured - first.captured) as f64 / seconds;
        let dropped_frames = last.dropped.saturating_sub(first.dropped);
        self.update_health(now, processed_fps, captured_fps, dropped_frames);

        Some(MonitorFps {
            processed_fps,
            captured_fps,
            source_fps: self.source_fps,
            dropped_frames,
            health: self.health,
        })
    }

    fn update_health(&mut self, now: Instant, processed_fps: f64, captured_fps: f64, dropped_frames: u64) {
        let below_capacity = captured_fps > 0.0 && processed_fps / captured_fps < MIN_PROCESSING_RATIO;
        if below_capacity {
            self.low_since.get_or_insert(now);
        } else {
            self.low_since = None;
        }
        let sustained_lag = self.low_since.is_some_and(|since| now.duration_since(since) >= LAG_DURATION);
        let raw = if dropped_frames >= 2 || sustained_lag {
            MonitorFpsHealth::Lagging
        } else if dropped_frames == 1 || below_capacity {
            MonitorFpsHealth::Warning
        } else {
            MonitorFpsHealth::Healthy
        };

        if raw == MonitorFpsHealth::Healthy && self.health != MonitorFpsHealth::Healthy {
            let since = *self.healthy_since.get_or_insert(now);
            if now.duration_since(since) >= RECOVERY_DURATION {
                self.health = MonitorFpsHealth::Healthy;
                self.healthy_since = None;
            }
        } else {
            self.healthy_since = None;
            self.health = raw;
        }
    }
}

#[cfg(test)]
#[path = "throughput_test.rs"]
mod throughput_test;
