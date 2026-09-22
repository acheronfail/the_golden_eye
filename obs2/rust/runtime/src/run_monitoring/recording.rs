//! Coordinates run detection, replay-buffer saves, and clip processing.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::{Instant, SystemTime};

use ge_catalog::run_catalog::RunCatalog;
use ge_cv::LevelMatch;
use serde::Deserialize;
use tokio::sync::broadcast;

use super::clip_output::{ClipOutputPolicy, configured_dir};
use super::clip_saving::SavePipeline;
use super::run_detection::{PendingSave, RunDetection, RunDetectionPolicy};
use super::{
    DEFAULT_CLIP_FILENAME_TEMPLATE,
    DEFAULT_POST_RUN_PADDING_SECS,
    DEFAULT_PRE_RUN_PADDING_SECS,
    DEFAULT_RECENT_RUN_LIMIT,
    MATCH_PADDING_BUFFER_SECS,
    RecordingStateEvent,
    RecordingStateStore,
    RecordingStatus,
};
use crate::app::AppEvent;
use crate::run_monitoring::publication::ReplaySaveStateStore;

/// Recording behaviour loaded when a monitor session starts. The saveable-clip
/// count is updated live; other options remain fixed for the session.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(rename_all = "camelCase")]
pub struct RecordingOptions {
    pub completed_output_path: String,
    pub recent_run_limit: usize,
    pub clip_filename_template: String,
    pub pre_run_padding_secs: f64,
    pub post_run_padding_secs: f64,
}

impl Default for RecordingOptions {
    fn default() -> Self {
        RecordingOptions {
            completed_output_path: String::new(),
            recent_run_limit: DEFAULT_RECENT_RUN_LIMIT,
            clip_filename_template: DEFAULT_CLIP_FILENAME_TEMPLATE.to_owned(),
            pre_run_padding_secs: DEFAULT_PRE_RUN_PADDING_SECS,
            post_run_padding_secs: DEFAULT_POST_RUN_PADDING_SECS,
        }
    }
}

impl RecordingOptions {
    fn non_negative_secs(value: f64, fallback: f64) -> f64 {
        if value.is_finite() { value.max(0.0) } else { fallback }
    }

    pub(super) fn detection_policy(&self) -> RunDetectionPolicy {
        RunDetectionPolicy {
            pre_run_padding_secs: Self::non_negative_secs(self.pre_run_padding_secs, 0.0) + MATCH_PADDING_BUFFER_SECS,
            post_run_padding_secs: Self::non_negative_secs(self.post_run_padding_secs, DEFAULT_POST_RUN_PADDING_SECS)
                + MATCH_PADDING_BUFFER_SECS,
        }
    }

    pub(super) fn output_policy(&self) -> ClipOutputPolicy {
        let trimmed = self.clip_filename_template.trim();
        ClipOutputPolicy {
            output_directory: configured_dir(&self.completed_output_path),
            filename_template: if trimmed.is_empty() {
                DEFAULT_CLIP_FILENAME_TEMPLATE.to_owned()
            } else {
                trimmed.to_owned()
            },
        }
    }
}

/// Metadata shared by runs finalized during one monitoring session.
pub struct RecordingSessionContext {
    pub(super) source_name: String,
    game_language: String,
    pub(super) monitor_session_id: Option<String>,
}

impl RecordingSessionContext {
    pub fn new(source_name: String, game_language: String, monitor_session_id: Option<String>) -> Self {
        Self { source_name, game_language, monitor_session_id }
    }
}

/// Tracks one recording session as it moves through the on-screen states, and
/// drives replay-buffer saves when runs finish. Applies decisions from [`RunDetection`].
pub struct RunRecorder {
    pub(super) run_detection: RunDetection,
    /// Normalized timing policy fixed for this monitor session.
    pub(super) detection_policy: RunDetectionPolicy,
    pub(super) save_pipeline: SavePipeline,
}

impl RunRecorder {
    pub fn new(
        event_tx: broadcast::Sender<AppEvent>,
        recording_state: RecordingStateStore,
        replay_saves: ReplaySaveStateStore,
        options: RecordingOptions,
        session: RecordingSessionContext,
        run_catalog: Arc<RunCatalog>,
    ) -> Self {
        let detection_policy = options.detection_policy();
        let output_policy = options.output_policy();
        let run_detection = RunDetection::new(session.game_language.clone());
        RunRecorder {
            run_detection,
            detection_policy,
            save_pipeline: SavePipeline::new(
                event_tx,
                recording_state,
                replay_saves,
                output_policy,
                options.recent_run_limit,
                session,
                run_catalog,
            ),
        }
    }

    pub(super) fn on_frame(&mut self, now: Instant, matched: &LevelMatch) {
        let update = self.run_detection.on_frame(now, SystemTime::now(), matched, self.detection_policy);
        if update.ensure_replay_buffer {
            #[cfg(not(test))]
            super::replay_buffer::REPLAY_BUFFER.ensure_replay_buffer_running();
        }
        for pending in update.ready {
            self.flush_ready(pending, now);
        }
        self.sync_pending_event(now, update.pending_changed);
        if let Some(phase) = update.phase {
            self.emit(phase);
        }
        self.poll_pending(now);
    }

    pub(super) fn set_game_language(&mut self, language: String) {
        self.run_detection.set_game_language(language);
    }

    pub fn set_recent_run_limit_source(&mut self, source: Arc<AtomicUsize>) {
        self.save_pipeline.set_recent_run_limit_source(source);
    }

    /// Publish the phase and attach its generation to the pending save, so a late
    /// completion clears only its own transition.
    fn emit(&mut self, status: RecordingStatus) {
        let generation = self.save_pipeline.recording_state.handle(RecordingStateEvent::PhaseChanged(status));
        if matches!(status, RecordingStatus::SavePending | RecordingStatus::StatsSkipped)
            && let Some(pending) = self.run_detection.pending.as_mut()
        {
            pending.phase_generation = Some(generation);
        }
    }

    /// Show the provisional row once and refresh it when the voted time changes.
    pub(super) fn sync_pending_event(&mut self, now: Instant, time_changed: bool) {
        let Some(pending) = self.run_detection.pending.as_ref() else {
            return;
        };
        if !pending.pending_event_sent || time_changed {
            self.save_pipeline.publish_pending(pending, self.detection_policy, now);
            self.run_detection.pending.as_mut().unwrap().pending_event_sent = true;
        }
    }

    /// Save and trim the pending clip asynchronously, if any.
    fn flush_pending(&mut self, now: Instant) {
        if let Some(pending) = self.run_detection.pending.take() {
            self.save_pipeline.spawn(pending, now, self.detection_policy);
        }
    }

    pub(super) fn flush_ready(&self, pending: PendingSave, now: Instant) {
        self.save_pipeline.spawn(pending, now, self.detection_policy);
    }

    /// When the in-flight save is due to fire, or `None` when nothing is pending.
    /// The monitor loop waits on this so the save fires on time even if captured
    /// frames stop arriving (e.g. a paused source).
    pub fn pending_fire_at(&self) -> Option<Instant> {
        self.run_detection.pending.as_ref().map(|pending| pending.fire_at)
    }

    /// Fire the scheduled save once its post-run padding window has elapsed. Safe
    /// to call on any tick (frame or idle wakeup); a no-op until then.
    pub(super) fn poll_pending(&mut self, now: Instant) {
        if self.run_detection.pending.as_ref().is_some_and(|pending| now >= pending.fire_at) {
            self.flush_pending(now);
        }
    }

    /// Save and trim the pending clip synchronously during shutdown, preserving
    /// the scheduled post-run padding window before OBS is asked to save.
    #[cfg(not(test))]
    pub(super) fn flush_pending_on_shutdown(&mut self) {
        if let Some(pending) = self.run_detection.pending.take() {
            self.save_pipeline.flush_on_shutdown(pending, Instant::now(), self.detection_policy);
        }
    }
}

#[cfg(not(test))]
impl Drop for RunRecorder {
    fn drop(&mut self) {
        self.flush_pending_on_shutdown();
    }
}

#[cfg(test)]
impl Drop for RunRecorder {
    fn drop(&mut self) {
        assert!(self.run_detection.pending.is_none(), "test dropped RunRecorder with a pending save");
    }
}
