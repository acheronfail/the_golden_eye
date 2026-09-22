use crate::config::MonitorTimingMode;
use crate::obs::frame_capture::CapturedFrameStats;

pub(super) struct MonitorTiming {
    mode: MonitorTimingMode,
    slow_ms: f64,
    last_dropped_frames_total: u64,
}

impl MonitorTiming {
    pub(super) fn new(source_fps: f64, mode: MonitorTimingMode) -> Self {
        let slow_ms = crate::config::default_monitor_slow_ms(source_fps);

        Self { mode, slow_ms, last_dropped_frames_total: 0 }
    }

    pub(super) fn enabled(&self) -> bool {
        self.mode != MonitorTimingMode::Off
    }

    pub(super) fn observe(
        &mut self,
        stats: CapturedFrameStats,
        match_ms: Option<f64>,
        cv_runtime_ms: Option<f64>,
        source_fps: f64,
    ) {
        if self.mode == MonitorTimingMode::Off {
            return;
        }
        let (Some(capture_ms), Some(capture_timings), Some(mailbox_wait_ms), Some(match_ms)) =
            (stats.capture_ms, stats.capture_timings, stats.mailbox_wait_ms, match_ms)
        else {
            return;
        };

        let dropped_frames = stats.dropped_frames_total.saturating_sub(self.last_dropped_frames_total);
        self.last_dropped_frames_total = stats.dropped_frames_total;
        let total_ms = capture_ms + mailbox_wait_ms + match_ms;
        let slow = total_ms >= self.slow_ms || dropped_frames > 0;

        if slow {
            tracing::warn!(
                callback_interval_ms = stats.callback_interval_ms,
                capture_ms,
                capture_source_ms = capture_timings.source_ms,
                capture_allocation_ms = capture_timings.allocation_ms,
                capture_render_stage_ms = capture_timings.render_stage_ms,
                capture_map_copy_ms = capture_timings.map_copy_ms,
                capture_cleanup_ms = capture_timings.cleanup_ms,
                mailbox_wait_ms,
                match_ms,
                cv_runtime_ms,
                total_ms,
                dropped_frames,
                dropped_frames_total = stats.dropped_frames_total,
                source_fps,
                slow_threshold_ms = self.slow_ms,
                "monitor frame timing"
            );
        } else if self.mode == MonitorTimingMode::Verbose {
            tracing::info!(
                callback_interval_ms = stats.callback_interval_ms,
                capture_ms,
                capture_source_ms = capture_timings.source_ms,
                capture_allocation_ms = capture_timings.allocation_ms,
                capture_render_stage_ms = capture_timings.render_stage_ms,
                capture_map_copy_ms = capture_timings.map_copy_ms,
                capture_cleanup_ms = capture_timings.cleanup_ms,
                mailbox_wait_ms,
                match_ms,
                cv_runtime_ms,
                total_ms,
                dropped_frames,
                dropped_frames_total = stats.dropped_frames_total,
                source_fps,
                slow_threshold_ms = self.slow_ms,
                "monitor frame timing"
            );
        }
    }
}
