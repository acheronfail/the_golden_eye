//! Coordinates frame detection, recording, and retained monitor updates.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use ge_cv::{WatchDetector, detect_black_frame, detect_watch};
use tokio::sync::broadcast;

use super::matcher::{MonitorMatcher, switch_detected_language};
use super::session::{RunSession, unix_time_ms};
use super::throughput::ThroughputMeter;
use super::timing::MonitorTiming;
use crate::app::AppEvent;
use crate::config::MonitorTimingMode;
use crate::obs::frame_capture::{Captured, ObsSource};

pub(super) struct FrameWorker {
    matcher: MonitorMatcher,
    event_tx: broadcast::Sender<AppEvent>,
    annotations_enabled: Arc<AtomicBool>,
    active_lang: String,
    last_diagnostics_enabled: bool,
    throughput: ThroughputMeter,
    monitor_timing: MonitorTiming,
    source_fps: f64,
    watch_detector: WatchDetector,
}

impl FrameWorker {
    pub(super) fn new(
        matcher: MonitorMatcher,
        event_tx: broadcast::Sender<AppEvent>,
        annotations_enabled: Arc<AtomicBool>,
        source_fps: f64,
        timing_mode: MonitorTimingMode,
    ) -> Self {
        Self {
            matcher,
            event_tx,
            annotations_enabled,
            active_lang: super::DEFAULT_MONITOR_LANGUAGE.to_owned(),
            last_diagnostics_enabled: false,
            throughput: ThroughputMeter::new(Instant::now(), source_fps),
            monitor_timing: MonitorTiming::new(source_fps, timing_mode),
            source_fps,
            watch_detector: WatchDetector::default(),
        }
    }

    pub(super) fn run(mut self, mut source: ObsSource, session: &mut RunSession) {
        loop {
            let diagnostics_enabled = self.annotations_enabled.load(Ordering::Acquire);
            if diagnostics_enabled != self.last_diagnostics_enabled {
                self.last_diagnostics_enabled = diagnostics_enabled;
                session.refresh_display();
            }
            self.matcher.set_diagnostics(diagnostics_enabled);
            // Wake by the pending save's fire time even if no frame arrives, so a
            // paused/stalled source can't stall (and eventually roll out of the
            // replay buffer) a scheduled save.
            let deadline = session.pending_fire_at();
            let (result, black_frame, watch_signal, observed_at_unix_ms, match_ms, stats) = match source
                .capture_with_stats_until(deadline, |bytes, w, h| {
                    let observed_at_unix_ms = unix_time_ms();
                    let match_started = self.monitor_timing.enabled().then(Instant::now);
                    let result = self.matcher.match_frame(bytes, w, h);
                    let active_picture = self.matcher.active_picture_region(w, h);
                    let black_frame = detect_black_frame(bytes, w, h, active_picture);
                    let watch_signal = detect_watch(bytes, w, h, active_picture);
                    let match_ms = match_started.map(|started| started.elapsed().as_secs_f64() * 1000.0);
                    (result, black_frame, watch_signal, observed_at_unix_ms, match_ms)
                }) {
                Captured::Frame((result, black_frame, watch_signal, observed_at_unix_ms, match_ms), stats) => {
                    (result, black_frame, watch_signal, observed_at_unix_ms, match_ms, stats)
                }
                Captured::Idle => {
                    session.poll_pending(Instant::now());
                    continue;
                }
                Captured::Closed => break,
            };
            let now = Instant::now();
            if let Some(fps) = self.throughput.observe(now, stats.dropped_frames_total) {
                let _ = self.event_tx.send(AppEvent::MonitorFps(fps));
            }

            // Once the matcher has calibrated this source's aspect, hand the
            // transform to the capture layer so subsequent frames are cropped +
            // un-stretched on the GPU at capture time.
            source.set_capture_region(self.matcher.capture_region());

            match result {
                Ok(info) => {
                    self.monitor_timing.observe(stats, match_ms, Some(info.runtime_ms), self.source_fps);
                    tracing::debug!(?info);
                    if switch_detected_language(&info, &mut self.matcher, &mut self.active_lang, |lang| {
                        Ok(MonitorMatcher::from_env(lang)?.with_diagnostics(diagnostics_enabled))
                    }) {
                        session.set_language(self.active_lang.clone());
                    }

                    session.process_match(now, info);
                }
                Err(e) => {
                    self.monitor_timing.observe(stats, match_ms, None, self.source_fps);
                    tracing::error!("err: {}", e.message);
                }
            }
            if let Some(signal) = black_frame {
                session.observe_black_frame(signal, unix_time_ms());
            }
            if let Some(signal) = watch_signal
                && let Some(transition) = self.watch_detector.observe(signal).transition
            {
                session.observe_watch(transition, observed_at_unix_ms);
            }
        }
        tracing::info!("monitor loop exiting");
    }
}
