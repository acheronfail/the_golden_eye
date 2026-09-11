//! Coordinates frame detection, recording, and retained monitor updates.

use std::sync::atomic::Ordering;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::capture::{Captured, ObsSource};
use super::clocks::MonitorClockStore;
use super::matcher::{DisplayTimeSmoother, MonitorMatcher, log_level_match, switch_detected_language};
use super::throughput::ThroughputMeter;
use super::timing::MonitorTiming;
use crate::config::MonitorTimingMode;
use crate::cv::{LevelMatch, WatchDetector, detect_black_frame, detect_watch};
use crate::http::{AppEvent, AppState};
use crate::recording::RecordingState;

pub(super) struct MonitorSession {
    clocks: MonitorClockStore,
    matcher: MonitorMatcher,
    recording: RecordingState,
    state: AppState,
    active_lang: String,
    last: Option<LevelMatch>,
    display_smoother: DisplayTimeSmoother,
    last_diagnostics_enabled: bool,
    throughput: ThroughputMeter,
    monitor_timing: MonitorTiming,
    source_fps: f64,
    watch_detector: WatchDetector,
}

impl MonitorSession {
    pub(super) fn new(
        matcher: MonitorMatcher,
        recording: RecordingState,
        state: AppState,
        source_fps: f64,
        timing_mode: MonitorTimingMode,
        clocks: MonitorClockStore,
    ) -> Self {
        Self {
            clocks,
            matcher,
            recording,
            state,
            active_lang: super::DEFAULT_MONITOR_LANGUAGE.to_owned(),
            last: None,
            display_smoother: DisplayTimeSmoother::new(),
            last_diagnostics_enabled: false,
            throughput: ThroughputMeter::new(Instant::now(), source_fps),
            monitor_timing: MonitorTiming::new(source_fps, timing_mode),
            source_fps,
            watch_detector: WatchDetector::default(),
        }
    }

    pub(super) fn run(mut self, mut source: ObsSource) {
        loop {
            let diagnostics_enabled = self.state.monitor_annotations_enabled.load(Ordering::Acquire);
            if diagnostics_enabled != self.last_diagnostics_enabled {
                self.last_diagnostics_enabled = diagnostics_enabled;
                self.last = None;
            }
            self.matcher.set_diagnostics(diagnostics_enabled);
            // Wake by the pending save's fire time even if no frame arrives, so a
            // paused/stalled source can't stall (and eventually roll out of the
            // replay buffer) a scheduled save.
            let deadline = self.recording.pending_fire_at();
            let (result, black_frame, watch_signal, observed_at_unix_ms, match_ms, stats) = match source
                .capture_with_stats_until(deadline, |bytes, w, h| {
                    let observed_at_unix_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                        .try_into()
                        .unwrap_or(u64::MAX);
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
                    self.recording.poll_pending(Instant::now());
                    continue;
                }
                Captured::Closed => break,
            };
            let now = Instant::now();
            if let Some(fps) = self.throughput.observe(now, stats.dropped_frames_total) {
                let _ = self.state.event_tx.send(AppEvent::MonitorFps(fps));
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
                        self.state.snapshot.set_monitor_language(self.active_lang.clone());
                        self.recording.set_game_language(self.active_lang.clone());
                        self.last = None;
                    }

                    // The recorder votes over raw per-frame readings itself, so it
                    // must see the unsmoothed match; only the live display is voted.
                    self.recording.on_frame(now, &info);
                    let mut display = info;
                    display.times = self.display_smoother.smooth(&display);
                    let changed = self.last.as_ref().is_none_or(|prev| !prev.same_state(&display));
                    if changed {
                        log_level_match(&display);
                        self.last = Some(display.clone());
                        self.clocks.observe_match(display);
                    }
                }
                Err(e) => {
                    self.monitor_timing.observe(stats, match_ms, None, self.source_fps);
                    tracing::error!("err: {}", e.message);
                }
            }
            if let Some(signal) = black_frame {
                self.clocks.observe_black_frame(signal);
            }
            if let Some(signal) = watch_signal
                && let Some(transition) = self.watch_detector.observe(signal).transition
            {
                self.clocks.observe_watch_transition(transition, observed_at_unix_ms);
            }
        }
        tracing::info!("monitor loop exiting");
    }
}
