//! Coordinates run detection, replay-buffer saves, and clip processing.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
#[cfg(test)]
use std::time::Duration;
use std::time::{Instant, SystemTime};

pub use ge_settings::{
    DEFAULT_CLIP_FILENAME_TEMPLATE,
    DEFAULT_POST_RUN_PADDING_SECS,
    DEFAULT_PRE_RUN_PADDING_SECS,
    DEFAULT_RECENT_RUN_LIMIT,
    MAX_RECENT_RUN_LIMIT,
};
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::cv::LevelMatch;
use crate::db::run_catalog::RunCatalog;
use crate::http::{AppEvent, RecordingStateStore, RecordingStatus, ReplaySaveStateStore};

mod clip_output;
mod replay_buffer;
mod replay_coordinator;
mod save_pipeline;
mod tracker;

use clip_output::{ClipOutputPolicy, configured_dir};
use replay_buffer::ensure_replay_buffer_running_for_recording;
pub use replay_buffer::{
    ensure_replay_buffer_running,
    on_replay_buffer_started,
    on_replay_buffer_starting,
    on_replay_buffer_stopped,
    on_replay_buffer_stopping,
    on_replay_saved,
    replay_buffer_active,
    replay_buffer_available,
    replay_buffer_enabled,
    replay_buffer_max_seconds,
    replay_buffer_output_directory,
    stop_replay_buffer_if_active,
};
#[cfg(test)]
use save_pipeline::SaveAndTrimJob;
use save_pipeline::SavePipeline;
#[cfg(test)]
use tracker::TrackerUpdate;
use tracker::{PendingSave, RunTracker, RunTrackerPolicy};

/// Internal safety margin added to both the pre- and post-run padding, on top of
/// the user's configured values and hidden from them, so a single-frame timing
/// window can't drop the level-start briefing or stats overlay (e.g. padding 0).
const MATCH_PADDING_BUFFER_SECS: f64 = 0.5;

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

    fn tracker_policy(&self) -> RunTrackerPolicy {
        RunTrackerPolicy {
            pre_run_padding_secs: Self::non_negative_secs(self.pre_run_padding_secs, 0.0) + MATCH_PADDING_BUFFER_SECS,
            post_run_padding_secs: Self::non_negative_secs(self.post_run_padding_secs, DEFAULT_POST_RUN_PADDING_SECS)
                + MATCH_PADDING_BUFFER_SECS,
        }
    }

    fn output_policy(&self) -> ClipOutputPolicy {
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
    source_name: String,
    game_language: String,
    monitor_session_id: Option<String>,
}

impl RecordingSessionContext {
    pub fn new(source_name: String, game_language: String, monitor_session_id: Option<String>) -> Self {
        Self { source_name, game_language, monitor_session_id }
    }
}

/// Tracks one recording session as it moves through the on-screen states, and
/// drives replay-buffer saves when runs finish. Fed via [`RecordingState::on_frame`].
pub struct RecordingState {
    tracker: RunTracker,
    /// Normalized timing policy fixed for this monitor session.
    tracker_policy: RunTrackerPolicy,
    save_pipeline: SavePipeline,
}

impl RecordingState {
    pub fn new(
        event_tx: broadcast::Sender<AppEvent>,
        recording_state: RecordingStateStore,
        replay_saves: ReplaySaveStateStore,
        options: RecordingOptions,
        session: RecordingSessionContext,
        run_catalog: Arc<RunCatalog>,
    ) -> Self {
        let tracker_policy = options.tracker_policy();
        let output_policy = options.output_policy();
        let tracker = RunTracker::new(session.game_language.clone());
        RecordingState {
            tracker,
            tracker_policy,
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

    pub fn set_recent_run_limit_source(&mut self, source: Arc<AtomicUsize>) {
        self.save_pipeline.set_recent_run_limit_source(source);
    }

    /// Publish a recorder state transition to the backend-retained phase store
    /// Event-stream clients see it in the next app snapshot.
    /// For `SavePending`/`StatsSkipped`, records the generation on the pending
    /// save so its completion/discard can clear that exact transition later.
    fn emit(&mut self, status: RecordingStatus) {
        let generation = self.save_pipeline.recording_state.set(status);
        if matches!(status, RecordingStatus::SavePending | RecordingStatus::StatsSkipped)
            && let Some(pending) = self.tracker.pending.as_mut()
        {
            pending.phase_generation = Some(generation);
        }
    }

    /// Update the game/template language attached to future clip metadata. Used
    /// when monitor language auto-correction detects the other game language.
    pub fn set_game_language(&mut self, game_language: String) {
        self.tracker.set_game_language(game_language);
    }

    #[cfg(test)]
    fn schedule_save(&mut self, now: Instant, clip_start: Instant, stats: Option<LevelMatch>) -> bool {
        let mut update = TrackerUpdate::default();
        self.tracker.schedule_save(now, SystemTime::now(), clip_start, stats, self.tracker_policy, &mut update);
        for pending in update.ready {
            self.flush_ready(pending, now);
        }
        self.sync_pending_event(now, update.pending_changed);
        true
    }

    /// Show the provisional row once and refresh it when the voted time changes.
    fn sync_pending_event(&mut self, now: Instant, time_changed: bool) {
        let Some(pending) = self.tracker.pending.as_ref() else {
            return;
        };
        if !pending.pending_event_sent || time_changed {
            self.save_pipeline.publish_pending(pending, self.tracker_policy, now);
            self.tracker.pending.as_mut().unwrap().pending_event_sent = true;
        }
    }

    /// Build a save+trim job for the pending clip, if any, anchored to `now` as
    /// the save moment (the saved file ends at ~now, so the run is its final
    /// `elapsed` seconds). A no-op when nothing is pending.
    #[cfg(test)]
    fn take_pending_job(&mut self, now: Instant) -> Option<SaveAndTrimJob> {
        let pending = self.tracker.pending.take()?;
        Some(self.save_pipeline.job(pending, now, self.tracker_policy))
    }

    /// Save and trim the pending clip asynchronously, if any.
    fn flush_pending(&mut self, now: Instant) {
        if let Some(pending) = self.tracker.pending.take() {
            self.save_pipeline.spawn(pending, now, self.tracker_policy);
        }
    }

    fn flush_ready(&self, pending: PendingSave, now: Instant) {
        self.save_pipeline.spawn(pending, now, self.tracker_policy);
    }

    /// When the in-flight save is due to fire, or `None` when nothing is pending.
    /// The monitor loop waits on this so the save fires on time even if captured
    /// frames stop arriving (e.g. a paused source).
    pub fn pending_fire_at(&self) -> Option<Instant> {
        self.tracker.pending.as_ref().map(|pending| pending.fire_at)
    }

    /// Fire the scheduled save once its post-run padding window has elapsed. Safe
    /// to call on any tick (frame or idle wakeup); a no-op until then.
    pub fn poll_pending(&mut self, now: Instant) {
        if self.tracker.pending.as_ref().is_some_and(|pending| now >= pending.fire_at) {
            self.flush_pending(now);
        }
    }

    /// Save and trim the pending clip synchronously during shutdown, preserving
    /// the scheduled post-run padding window before OBS is asked to save.
    #[cfg(not(test))]
    fn flush_pending_on_shutdown(&mut self) {
        if let Some(pending) = self.tracker.pending.take() {
            self.save_pipeline.flush_on_shutdown(pending, Instant::now(), self.tracker_policy);
        }
    }

    #[cfg(test)]
    fn flush_pending_on_shutdown_with(
        &mut self,
        now: Instant,
        sleep: impl FnOnce(Duration),
        save: impl FnOnce(SaveAndTrimJob),
    ) {
        let Some(pending) = self.tracker.pending.take() else {
            return;
        };
        self.save_pipeline.flush_on_shutdown_with(pending, now, self.tracker_policy, sleep, save);
    }

    /// Feed the latest matched frame (and the current time). Called once per
    /// captured frame, so it also polls the pending-save timer.
    pub fn on_frame(&mut self, now: Instant, m: &LevelMatch) {
        let update = self.tracker.on_frame(now, SystemTime::now(), m, self.tracker_policy);
        if update.ensure_replay_buffer {
            ensure_replay_buffer_running_for_recording();
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
}

#[cfg(not(test))]
impl Drop for RecordingState {
    fn drop(&mut self) {
        self.flush_pending_on_shutdown();
    }
}

#[cfg(test)]
impl Drop for RecordingState {
    fn drop(&mut self) {
        assert!(self.tracker.pending.is_none(), "test dropped RecordingState with a pending save");
    }
}

#[cfg(test)]
#[path = "tests/support.rs"]
mod test_support;
