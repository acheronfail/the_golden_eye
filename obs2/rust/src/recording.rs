//! Replay-buffer driven recording.
//!
//! Rather than start/stop a fresh recording per run (the legacy approach, which
//! risked clipping the start while the recorder spun up), we keep OBS's replay
//! buffer running for the whole session and save a window out of it at the end.
//! [`RecordingState`] is fed every matched frame; it tracks where a run begins
//! and ends, waits for the configured post-run padding, saves the replay buffer,
//! and trims it (via [`crate::ffmpeg`]) down to just the run.
//!
//! Timing is anchored to the moment the buffer is saved: the saved file ends at
//! ~"now", so the configured pre/post padding is translated into offsets from
//! the end of that saved file.

use std::ffi::{CStr, c_char};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::cv::{LevelMatch, Screen};
use crate::http::{MonitorEvent, RecordingSavePending, RecordingSaved, RecordingStateStore, RecordingStatus};
use crate::{ffmpeg, ge};

/// Default filename template for trimmed clips. Mirrors the frontend default and
/// falls back through the unique-name suffixer when multiple runs render alike.
pub const DEFAULT_CLIP_FILENAME_TEMPLATE: &str = "{level} - {time} - {difficulty} - {status}";
pub const DEFAULT_PRE_RUN_PADDING_SECS: f64 = 5.0;
pub const DEFAULT_POST_RUN_PADDING_SECS: f64 = 5.0;
pub const DEFAULT_MINIMUM_FAILED_RUN_LENGTH_SECS: f64 = 10.0;
const PRE_RUN_MATCH_BUFFER_SECS: f64 = 0.5;

/// How long to wait for OBS to finish writing the saved replay file before
/// giving up. The save is asynchronous; we block on the replay-saved event
/// (delivered via [`on_replay_saved`]) rather than polling.
#[cfg(not(test))]
const REPLAY_SAVE_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a monitor start should wait for OBS to finish an in-progress replay
/// buffer stop before giving up.
const REPLAY_STOP_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a monitor start should wait for OBS to make the replay buffer active
/// after `obs_frontend_replay_buffer_start`.
const REPLAY_START_TIMEOUT: Duration = Duration::from_secs(2);
const REPLAY_START_RETRIES: usize = 4;
const REPLAY_START_RETRY_DELAY: Duration = Duration::from_millis(250);
/// OBS can ignore a replay-buffer start issued immediately after the stopped
/// event. Give the frontend a brief turn to finish its state transition.
const REPLAY_STOP_SETTLE_DELAY: Duration = Duration::from_millis(400);
const OBS_OUTPUT_PATH_BUFFER_SIZE: usize = 4096;

/// Recording behaviour supplied by the frontend when a monitor session starts.
/// The settings store materializes empty output paths into runtime defaults
/// before these options are read.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RecordingOptions {
    pub completed_output_path: String,
    pub save_failed_runs: bool,
    pub failed_output_path: String,
    /// Number of failed clips to keep in the failed output directory. 0 means
    /// unlimited.
    pub failed_run_limit: usize,
    /// Minimum run length required before a failed run is saved. 0 saves every
    /// failed run. Uses stats-screen time when present, otherwise detected
    /// start-to-end time. This excludes pre/post padding.
    pub minimum_failed_run_length_secs: f64,
    pub clip_filename_template: String,
    pub pre_run_padding_secs: f64,
    pub post_run_padding_secs: f64,
}

impl Default for RecordingOptions {
    fn default() -> Self {
        RecordingOptions {
            completed_output_path: String::new(),
            save_failed_runs: true,
            failed_output_path: String::new(),
            failed_run_limit: 0,
            minimum_failed_run_length_secs: DEFAULT_MINIMUM_FAILED_RUN_LENGTH_SECS,
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

    fn clip_filename_template(&self) -> &str {
        let trimmed = self.clip_filename_template.trim();
        if trimmed.is_empty() { DEFAULT_CLIP_FILENAME_TEMPLATE } else { trimmed }
    }

    fn pre_run_padding_secs(&self) -> f64 {
        Self::non_negative_secs(self.pre_run_padding_secs, 0.0) + PRE_RUN_MATCH_BUFFER_SECS
    }

    fn post_run_padding_secs(&self) -> f64 {
        Self::non_negative_secs(self.post_run_padding_secs, DEFAULT_POST_RUN_PADDING_SECS)
    }

    fn minimum_failed_run_length_secs(&self) -> f64 {
        Self::non_negative_secs(self.minimum_failed_run_length_secs, 0.0)
    }

    fn save_delay(&self) -> Duration {
        Duration::from_secs_f64(self.post_run_padding_secs())
    }
}

/// The latest replay-saved event, published by the OBS frontend callback (see
/// [`on_replay_saved`]) and awaited by the save thread. `generation` ticks once
/// per event so a waiter can tell a fresh save from a stale one; `last_path` is
/// the file OBS just wrote (or `None` if it reported none).
struct ReplaySaved {
    generation: u64,
    last_path: Option<String>,
}

static REPLAY_SAVED: Mutex<ReplaySaved> = Mutex::new(ReplaySaved { generation: 0, last_path: None });
static REPLAY_SAVED_CV: Condvar = Condvar::new();

struct ReplayBufferLifecycle {
    starting: bool,
    stopping: bool,
    last_stopped_at: Option<Instant>,
}

static REPLAY_BUFFER_LIFECYCLE: Mutex<ReplayBufferLifecycle> =
    Mutex::new(ReplayBufferLifecycle { starting: false, stopping: false, last_stopped_at: None });
static REPLAY_BUFFER_LIFECYCLE_CV: Condvar = Condvar::new();
static REPLAY_BUFFER_ENSURE: Mutex<()> = Mutex::new(());

/// Publish a replay-saved event and wake any waiting save thread. Called (via
/// the `ge_replay_buffer_saved` FFI export) from the OBS frontend event
/// callback when `OBS_FRONTEND_EVENT_REPLAY_BUFFER_SAVED` fires.
pub fn on_replay_saved(path: Option<String>) {
    let mut guard = REPLAY_SAVED.lock().unwrap_or_else(|p| p.into_inner());
    guard.generation = guard.generation.wrapping_add(1);
    guard.last_path = path;
    drop(guard);
    REPLAY_SAVED_CV.notify_all();
}

/// Publish that OBS has begun starting the replay buffer.
pub fn on_replay_buffer_starting() {
    let mut guard = REPLAY_BUFFER_LIFECYCLE.lock().unwrap_or_else(|p| p.into_inner());
    if !guard.starting {
        tracing::debug!("replay buffer starting");
    }
    guard.starting = true;
    drop(guard);
    REPLAY_BUFFER_LIFECYCLE_CV.notify_all();
}

/// Publish that OBS has made the replay buffer active.
pub fn on_replay_buffer_started() {
    let mut guard = REPLAY_BUFFER_LIFECYCLE.lock().unwrap_or_else(|p| p.into_inner());
    if guard.starting {
        tracing::debug!("replay buffer started");
    }
    guard.starting = false;
    guard.last_stopped_at = None;
    drop(guard);
    REPLAY_BUFFER_LIFECYCLE_CV.notify_all();
}

/// Publish that OBS has begun stopping the replay buffer. This is also called
/// when we request a stop, because a quick monitor restart can reach
/// `/monitor/start` before OBS emits the frontend `STOPPING` event.
pub fn on_replay_buffer_stopping() {
    let mut guard = REPLAY_BUFFER_LIFECYCLE.lock().unwrap_or_else(|p| p.into_inner());
    if !guard.stopping {
        tracing::debug!("replay buffer stopping");
    }
    guard.starting = false;
    guard.stopping = true;
    guard.last_stopped_at = None;
    drop(guard);
    REPLAY_BUFFER_LIFECYCLE_CV.notify_all();
}

/// Publish that OBS has fully stopped the replay buffer and wake any monitor
/// start waiting to re-enable it.
pub fn on_replay_buffer_stopped() {
    let mut guard = REPLAY_BUFFER_LIFECYCLE.lock().unwrap_or_else(|p| p.into_inner());
    if guard.stopping {
        tracing::debug!("replay buffer stopped");
    }
    guard.starting = false;
    guard.stopping = false;
    guard.last_stopped_at = Some(Instant::now());
    drop(guard);
    REPLAY_BUFFER_LIFECYCLE_CV.notify_all();
}

/// The current event generation. Snapshotted *before* triggering a save so the
/// subsequent wait only resolves on a new event, never one already delivered.
#[cfg(not(test))]
fn replay_saved_generation() -> u64 {
    REPLAY_SAVED.lock().unwrap_or_else(|p| p.into_inner()).generation
}

/// Block until a replay-saved event newer than `since` arrives, returning the
/// path OBS wrote, or `None` on timeout (or if the event carried no path).
#[cfg(not(test))]
fn wait_for_replay_saved(since: u64, timeout: Duration) -> Option<String> {
    let start = Instant::now();
    let mut guard = REPLAY_SAVED.lock().unwrap_or_else(|p| p.into_inner());
    while guard.generation == since {
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return None;
        }
        let (next, res) = REPLAY_SAVED_CV.wait_timeout(guard, timeout - elapsed).unwrap_or_else(|p| p.into_inner());
        guard = next;
        if res.timed_out() {
            return None;
        }
    }
    guard.last_path.clone()
}

fn wait_for_replay_buffer_not_stopping(timeout: Duration) -> bool {
    let start = Instant::now();
    loop {
        let mut guard = REPLAY_BUFFER_LIFECYCLE.lock().unwrap_or_else(|p| p.into_inner());
        while guard.stopping {
            let elapsed = start.elapsed();
            if elapsed >= timeout {
                return false;
            }

            tracing::info!("waiting for replay buffer to finish stopping");
            let (next, res) =
                REPLAY_BUFFER_LIFECYCLE_CV.wait_timeout(guard, timeout - elapsed).unwrap_or_else(|p| p.into_inner());
            guard = next;
            if res.timed_out() && guard.stopping {
                return false;
            }
        }

        let settle_remaining =
            guard.last_stopped_at.and_then(|stopped_at| REPLAY_STOP_SETTLE_DELAY.checked_sub(stopped_at.elapsed()));
        drop(guard);

        if let Some(remaining) = settle_remaining {
            tracing::debug!(?remaining, "letting replay buffer stop settle before restart");
            std::thread::sleep(remaining);
            continue;
        }

        return true;
    }
}

fn wait_for_replay_buffer_active(timeout: Duration) -> bool {
    let start = Instant::now();
    let mut guard = REPLAY_BUFFER_LIFECYCLE.lock().unwrap_or_else(|p| p.into_inner());
    while !replay_buffer_active() {
        if guard.stopping {
            guard.starting = false;
            return false;
        }

        let elapsed = start.elapsed();
        if elapsed >= timeout {
            guard.starting = false;
            return false;
        }

        tracing::info!("waiting for replay buffer to start");
        let (next, res) =
            REPLAY_BUFFER_LIFECYCLE_CV.wait_timeout(guard, timeout - elapsed).unwrap_or_else(|p| p.into_inner());
        guard = next;
        if res.timed_out() && !replay_buffer_active() {
            guard.starting = false;
            return false;
        }
    }

    guard.starting = false;
    guard.last_stopped_at = None;
    true
}

/// Whether the replay buffer is enabled in the active profile (the OBS "Enable
/// Replay Buffer" checkbox). Distinct from [`replay_buffer_active`].
pub fn replay_buffer_enabled() -> bool {
    unsafe { crate::ffi::ge_obs_replay_buffer_enabled() }
}

/// Whether OBS currently exposes a replay-buffer output. This can be false even
/// when the checkbox is enabled, for output modes where OBS disables replay
/// buffer internally.
pub fn replay_buffer_available() -> bool {
    unsafe { crate::ffi::ge_obs_replay_buffer_available() }
}

/// Configured maximum replay-buffer duration in seconds.
pub fn replay_buffer_max_seconds() -> Option<u64> {
    let seconds = unsafe { crate::ffi::ge_obs_replay_buffer_max_seconds() };
    u64::try_from(seconds).ok()
}

/// Directory OBS is configured to write replay-buffer files into.
pub fn replay_buffer_output_directory() -> Option<PathBuf> {
    let mut buffer = vec![0 as c_char; OBS_OUTPUT_PATH_BUFFER_SIZE];
    let ok = unsafe { crate::ffi::ge_obs_replay_buffer_output_directory(buffer.as_mut_ptr(), buffer.len()) };
    if !ok {
        return None;
    }

    let path = unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy().trim().to_owned();
    if path.is_empty() { None } else { Some(PathBuf::from(path)) }
}

/// Whether the replay buffer output is currently running.
pub fn replay_buffer_active() -> bool {
    unsafe { crate::ffi::obs_frontend_replay_buffer_active() }
}

/// Start the replay buffer if it is available and not already running.
pub fn ensure_replay_buffer_running() -> bool {
    let _ensure_guard = REPLAY_BUFFER_ENSURE.lock().unwrap_or_else(|p| p.into_inner());

    if !wait_for_replay_buffer_not_stopping(REPLAY_STOP_TIMEOUT) {
        tracing::warn!("timed out waiting for replay buffer to stop");
        return false;
    }

    if !replay_buffer_available() {
        if replay_buffer_enabled() {
            tracing::warn!("replay buffer is enabled in OBS but unavailable with the current output settings");
        } else {
            tracing::warn!("replay buffer is not enabled in OBS; recording will not work");
        }
        return false;
    }
    if !replay_buffer_active() {
        for attempt in 1..=REPLAY_START_RETRIES {
            tracing::info!(attempt, "starting replay buffer");
            on_replay_buffer_starting();
            unsafe { crate::ffi::obs_frontend_replay_buffer_start() };
            if wait_for_replay_buffer_active(REPLAY_START_TIMEOUT) {
                return true;
            }
            tracing::warn!(attempt, "replay buffer did not become active after start request");
            std::thread::sleep(REPLAY_START_RETRY_DELAY);
        }
        return false;
    }
    true
}

#[cfg(not(test))]
fn ensure_replay_buffer_running_for_recording() -> bool {
    ensure_replay_buffer_running()
}

#[cfg(test)]
fn ensure_replay_buffer_running_for_recording() -> bool {
    true
}

/// Stop the replay buffer if it is currently running.
pub fn stop_replay_buffer_if_active() {
    if replay_buffer_active() {
        tracing::info!("stopping replay buffer");
        on_replay_buffer_stopping();
        unsafe { crate::ffi::obs_frontend_replay_buffer_stop() };
    }
}

/// A save that has been scheduled and *will* happen, captured in full the moment
/// the stats screen is seen. It is intentionally decoupled from the active-run
/// state below: once scheduled it owns everything it needs, so backing out to
/// the level grid or immediately starting another run can never drop it -- it
/// still fires on its own timer.
struct PendingSave {
    /// Identifier shared by the pending and saved WebSocket events.
    save_id: u64,
    /// When the post-run padding window elapses and we save the buffer.
    fire_at: Instant,
    /// When the run began -- the anchor for where the trimmed clip starts.
    clip_start: Instant,
    /// When the run ending was detected -- the anchor for post-run padding.
    finish_at: Instant,
    /// The final report status seen for the run (for naming/logging).
    status: RunStatus,
    /// Wall-clock time when the run ending was detected.
    completed_at: SystemTime,
    /// ROM/template language active when this save was scheduled.
    rom_language: String,
    /// The stats-screen match, kept for naming the output clip.
    stats: Option<LevelMatch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunStatus {
    Complete,
    Failed,
    Abort,
    Kia,
}

impl RunStatus {
    fn from_failure_screen(screen: Screen) -> Option<Self> {
        match screen {
            Screen::Failed => Some(RunStatus::Failed),
            Screen::Abort => Some(RunStatus::Abort),
            Screen::Kia => Some(RunStatus::Kia),
            _ => None,
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "complete" => Some(RunStatus::Complete),
            "failed" => Some(RunStatus::Failed),
            "abort" => Some(RunStatus::Abort),
            "kia" => Some(RunStatus::Kia),
            _ => None,
        }
    }

    fn is_failed(self) -> bool {
        !matches!(self, RunStatus::Complete)
    }

    fn as_str(self) -> &'static str {
        match self {
            RunStatus::Complete => "complete",
            RunStatus::Failed => "failed",
            RunStatus::Abort => "abort",
            RunStatus::Kia => "kia",
        }
    }
}

fn recording_save_pending_event(
    save_id: u64,
    save_delay: Duration,
    estimated_duration_secs: f64,
    status: RunStatus,
    stats: Option<&LevelMatch>,
) -> RecordingSavePending {
    let level_info = stats.and_then(|m| ge::level_info(m.mission, m.part));
    let times = stats.and_then(|m| m.times);

    RecordingSavePending {
        save_id,
        save_in_secs: save_delay.as_secs_f64(),
        estimated_duration_secs,
        failed: status.is_failed(),
        status: status.as_str().to_owned(),
        level: level_info.map(|info| info.name.to_owned()).unwrap_or_else(|| "unknown".to_owned()),
        level_number: level_info.map(|info| info.number),
        difficulty: stats.and_then(|m| ge::difficulty_name(m.difficulty)).map(str::to_owned),
        time_secs: times.map(|t| t.time),
        target_time_secs: times.and_then(|t| t.target_time),
        best_time_secs: times.and_then(|t| t.best_time),
        stats: stats.cloned(),
    }
}

fn failed_run_length_secs(now: Instant, clip_start: Instant, stats: Option<&LevelMatch>) -> f64 {
    stats
        .and_then(|m| m.times)
        .map(|times| times.time)
        .filter(|time| *time >= 0)
        .map(|time| time as f64)
        .unwrap_or_else(|| now.saturating_duration_since(clip_start).as_secs_f64())
}

/// Tracks one recording session as it moves through the on-screen states, and
/// drives the replay-buffer save + trim when a run finishes. Fed one matched
/// frame at a time via [`RecordingState::on_frame`].
pub struct RecordingState {
    /// When the currently-active run began, or `None` when no run is in
    /// progress. A scheduled save lives in `pending` instead, so it survives the
    /// active run ending.
    clip_start: Option<Instant>,
    /// The final report status seen during the active run. Tracked for
    /// naming/logging; the clip is saved either way.
    status: Option<RunStatus>,
    /// The post-mission report screen (Complete/Failed/Abort/KIA) match seen
    /// during the active run, or `None` if the run hasn't reached one yet.
    /// Presence means the run finished, so backing out to the level grid from
    /// the report screen (which bypasses the stats screen) still saves the clip;
    /// its absence means the run was abandoned mid-play, with nothing to save.
    /// Kept for naming the clip when the stats screen is skipped (report screens
    /// carry the mission header but no timed rows, so no time is recovered).
    report: Option<LevelMatch>,
    /// A scheduled save in flight, if any. Independent of the active run: once
    /// set it is always saved when its timer elapses, even if the user backs out
    /// or starts another run in the meantime.
    pending: Option<PendingSave>,
    /// Monotonic id assigned to the next scheduled save so frontend notifications
    /// can be replaced when that save completes.
    next_save_id: u64,
    /// Broadcasts a [`MonitorEvent::RecordingSaved`] to WebSocket clients once a
    /// clip is written. Cloned into each save thread.
    event_tx: broadcast::Sender<MonitorEvent>,
    /// Retained recorder phase reported to status/WebSocket clients.
    recording_state: RecordingStateStore,
    /// Recording/output options fixed for this monitor session.
    options: RecordingOptions,
    /// OBS source this monitor session records from, stored in clip metadata.
    source_name: String,
    /// ROM/template language this monitor session matches, stored in clip metadata.
    rom_language: String,
}

impl RecordingState {
    pub fn new(
        event_tx: broadcast::Sender<MonitorEvent>,
        recording_state: RecordingStateStore,
        options: RecordingOptions,
        source_name: String,
        rom_language: String,
    ) -> Self {
        RecordingState {
            clip_start: None,
            status: None,
            report: None,
            pending: None,
            next_save_id: 1,
            event_tx,
            recording_state,
            options,
            source_name,
            rom_language,
        }
    }

    /// Publish a recorder state transition to the backend-retained phase store.
    /// WebSocket clients receive the same retained value through the monitor
    /// route's watch subscription.
    fn emit(&self, status: RecordingStatus) {
        self.recording_state.set(status);
    }

    /// Update the ROM/template language attached to future clip metadata. Used
    /// when monitor language auto-correction detects the other ROM language.
    pub fn set_rom_language(&mut self, rom_language: String) {
        self.rom_language = rom_language;
    }

    /// Schedule the replay-buffer save for a finished run, ending the active
    /// run's report tracking. `stats` names the clip -- the stats-screen match
    /// on the normal path, or the report-screen match when the stats screen was
    /// skipped. Any earlier pending save is flushed first so it isn't dropped.
    fn schedule_save(&mut self, now: Instant, clip_start: Instant, stats: Option<LevelMatch>) -> bool {
        self.flush_pending(now);
        let status = self.status.unwrap_or(RunStatus::Complete);
        if status.is_failed() && !self.options.save_failed_runs {
            tracing::info!("failed run reached an ending screen but failed-run saving is disabled");
            self.status = None;
            self.report = None;
            return false;
        }
        let run_length_secs = now.saturating_duration_since(clip_start).as_secs_f64();
        let measured_failed_run_length_secs = failed_run_length_secs(now, clip_start, stats.as_ref());
        if status.is_failed() && measured_failed_run_length_secs < self.options.minimum_failed_run_length_secs() {
            tracing::info!(
                failed_run_length_secs = measured_failed_run_length_secs,
                minimum_failed_run_length_secs = self.options.minimum_failed_run_length_secs(),
                "failed run reached an ending screen but was shorter than the configured minimum"
            );
            self.status = None;
            self.report = None;
            return false;
        }
        let save_delay = self.options.save_delay();
        let save_id = self.next_save_id;
        self.next_save_id = self.next_save_id.saturating_add(1).max(1);
        let estimated_duration_secs =
            run_length_secs + self.options.pre_run_padding_secs() + self.options.post_run_padding_secs();
        let pending_event =
            recording_save_pending_event(save_id, save_delay, estimated_duration_secs, status, stats.as_ref());
        self.pending = Some(PendingSave {
            save_id,
            fire_at: now + save_delay,
            clip_start,
            finish_at: now,
            status,
            completed_at: SystemTime::now(),
            rom_language: self.rom_language.clone(),
            stats,
        });
        self.status = None;
        self.report = None;
        let _ = self.event_tx.send(MonitorEvent::RecordingSavePending(pending_event));
        tracing::info!(?save_delay, "recording save scheduled");
        true
    }

    /// Build a save+trim job for the pending clip, if any, anchored to `now` as
    /// the save moment (the saved file ends at ~now, so the run is its final
    /// `elapsed` seconds). A no-op when nothing is pending.
    fn take_pending_job(&mut self, now: Instant) -> Option<SaveAndTrimJob> {
        if let Some(pending) = self.pending.take() {
            let start_before_save_secs =
                now.saturating_duration_since(pending.clip_start).as_secs_f64() + self.options.pre_run_padding_secs();
            let finish_before_save_secs = now.saturating_duration_since(pending.finish_at).as_secs_f64();
            let trim_tail_secs = (finish_before_save_secs - self.options.post_run_padding_secs()).max(0.0);
            Some(SaveAndTrimJob {
                save_id: pending.save_id,
                start_before_save_secs,
                trim_tail_secs,
                status: pending.status,
                completed_at: pending.completed_at,
                stats: pending.stats,
                options: self.options.clone(),
                source_name: self.source_name.clone(),
                rom_language: pending.rom_language,
                event_tx: self.event_tx.clone(),
                recording_state: self.recording_state.clone(),
            })
        } else {
            None
        }
    }

    /// Save and trim the pending clip asynchronously, if any.
    fn flush_pending(&mut self, now: Instant) {
        if let Some(job) = self.take_pending_job(now) {
            spawn_save_and_trim(job);
        }
    }

    /// Save and trim the pending clip synchronously during shutdown, preserving
    /// the scheduled post-run padding window before OBS is asked to save.
    #[cfg(not(test))]
    fn flush_pending_on_shutdown(&mut self) {
        self.flush_pending_on_shutdown_with(Instant::now(), std::thread::sleep, save_and_trim);
    }

    fn flush_pending_on_shutdown_with(
        &mut self,
        now: Instant,
        sleep: impl FnOnce(Duration),
        save: impl FnOnce(SaveAndTrimJob),
    ) {
        let Some(fire_at) = self.pending.as_ref().map(|pending| pending.fire_at) else {
            return;
        };

        let save_at = if fire_at > now {
            sleep(fire_at.duration_since(now));
            fire_at
        } else {
            now
        };

        if let Some(job) = self.take_pending_job(save_at) {
            save(job);
        }
    }

    /// Feed the latest matched frame (and the current time). Called once per
    /// captured frame, so it also polls the pending-save timer.
    pub fn on_frame(&mut self, now: Instant, m: &LevelMatch) {
        match m.screen {
            // A run begins at the level-start briefing or the 007-options screen.
            // A pending save from a previous run is left alone -- it fires on its
            // own timer -- so a new run can start without disturbing it.
            Screen::Start | Screen::Opts007 => {
                if self.clip_start.is_none() {
                    self.clip_start = Some(now);
                    self.status = None;
                    self.report = None;
                    ensure_replay_buffer_running_for_recording();
                    tracing::info!("recording session started");
                    self.emit(RecordingStatus::Started);
                }
            }
            // Returning to the mission grid. What it means depends on whether the
            // run reached its post-mission report screen. A pending save from an
            // earlier run is deliberately untouched either way -- it fires on its
            // own timer below.
            Screen::Levels => {
                if let Some(start) = self.clip_start.take() {
                    if let Some(report) = self.report.take() {
                        // The report screen was shown, then the user pressed B to
                        // return to the grid -- bypassing the stats screen. The run
                        // still finished, so save the clip on the same post-run
                        // padding timer as the stats path, naming it from the report screen.
                        // `schedule_save` clears `status`, so capture it first.
                        let status = self.status.unwrap_or(RunStatus::Complete);
                        tracing::info!("stats screen skipped (report -> level select)");
                        let scheduled = self.schedule_save(now, start, Some(report));
                        // Backing out to the grid is the *normal* ending for a
                        // failed run, so don't flag "skipped stats" -- just move to
                        // the saving state. Only a completed run whose stats screen
                        // was bypassed counts as skipped.
                        self.emit(if scheduled {
                            if status.is_failed() {
                                RecordingStatus::SavePending
                            } else {
                                RecordingStatus::StatsSkipped
                            }
                        } else {
                            RecordingStatus::FailedDiscarded
                        });
                    } else {
                        // No report screen was seen: the run was abandoned mid-play,
                        // so there's nothing worth saving.
                        self.status = None;
                        tracing::info!("recording session abandoned (returned to level select)");
                        self.emit(RecordingStatus::Cancelled);
                    }
                }
            }
            // Failure report screens flag the active run (it still ends at stats)
            // and mark that the run reached its report screen. Emit only on the
            // first failure frame (the screen lingers across many frames) so
            // clients see one transition, not a stream; the specific screen picks
            // the status so the UI can name *why* the run ended.
            Screen::Failed | Screen::Abort | Screen::Kia => {
                if self.clip_start.is_some() {
                    self.report.get_or_insert_with(|| m.clone());
                    if !self.status.is_some_and(RunStatus::is_failed) {
                        self.status = RunStatus::from_failure_screen(m.screen);
                        self.emit(match m.screen {
                            Screen::Abort => RecordingStatus::Aborted,
                            Screen::Kia => RecordingStatus::Kia,
                            _ => RecordingStatus::Failed,
                        });
                    }
                }
            }
            // The mission-complete report screen: also marks the run as having
            // reached its report screen. Emit `Complete` once -- on the first
            // report frame of a clean run, or when it clears a failure flagged
            // earlier this run (so clients can leave the "failed" state). Later
            // complete frames (the screen lingers) don't re-emit.
            Screen::Complete => {
                if self.clip_start.is_some() {
                    let first_report = self.report.is_none();
                    self.report.get_or_insert_with(|| m.clone());
                    if first_report || self.status.is_some_and(RunStatus::is_failed) {
                        self.status = Some(RunStatus::Complete);
                        self.emit(RecordingStatus::Complete);
                    }
                }
            }
            // The stats screen ends the run: hand the active run to a pending save
            // scheduled a few seconds out (so the clip captures the overlay).
            // Taking `clip_start` ends the active run, so later stats frames don't
            // re-schedule and a fresh run can begin right away. Any save still
            // waiting from an earlier run is flushed first so it isn't dropped.
            Screen::Stats => {
                if let Some(start) = self.clip_start.take() {
                    tracing::info!("stats detected");
                    if self.schedule_save(now, start, Some(m.clone())) {
                        self.emit(RecordingStatus::SavePending);
                    } else {
                        self.emit(RecordingStatus::FailedDiscarded);
                    }
                }
            }
            _ => {}
        }

        // Fire the scheduled save once its post-run padding window elapses. This
        // runs every frame regardless of the current screen, so a pending save
        // completes even after the user backs out or starts another run.
        if let Some(pending) = &self.pending
            && now >= pending.fire_at
        {
            self.flush_pending(now);
        }
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
        assert!(self.pending.is_none(), "test dropped RecordingState with a pending save");
    }
}

/// Inputs for saving the replay buffer and trimming it to the run window on a
/// dedicated thread.
struct SaveAndTrimJob {
    save_id: u64,
    start_before_save_secs: f64,
    trim_tail_secs: f64,
    status: RunStatus,
    completed_at: SystemTime,
    stats: Option<LevelMatch>,
    options: RecordingOptions,
    source_name: String,
    rom_language: String,
    event_tx: broadcast::Sender<MonitorEvent>,
    recording_state: RecordingStateStore,
}

struct TrimClipRequest<'a> {
    save_id: u64,
    replay_path: &'a str,
    start_before_save_secs: f64,
    trim_tail_secs: f64,
    status: RunStatus,
    completed_at: SystemTime,
    stats: Option<LevelMatch>,
    options: &'a RecordingOptions,
    source_name: &'a str,
    rom_language: &'a str,
}

#[cfg(not(test))]
fn save_and_trim(job: SaveAndTrimJob) {
    // Snapshot the event generation before triggering the save so we only
    // wake on the event this save produces, not one already delivered.
    let since = replay_saved_generation();
    tracing::info!("saving replay buffer");
    unsafe { crate::ffi::obs_frontend_replay_buffer_save() };

    // Block on the OBS replay-saved event (no polling); it carries the path.
    let path = match wait_for_replay_saved(since, REPLAY_SAVE_TIMEOUT) {
        Some(path) => path,
        None => {
            tracing::error!("replay buffer save did not complete in time");
            return;
        }
    };

    match trim_clip(TrimClipRequest {
        save_id: job.save_id,
        replay_path: &path,
        start_before_save_secs: job.start_before_save_secs,
        trim_tail_secs: job.trim_tail_secs,
        status: job.status,
        completed_at: job.completed_at,
        stats: job.stats,
        options: &job.options,
        source_name: &job.source_name,
        rom_language: &job.rom_language,
    }) {
        Ok(saved) => {
            // Ignore send errors: with no WebSocket clients there are no
            // subscribers, but the save still succeeded.
            let _ = job.event_tx.send(MonitorEvent::RecordingSaved(saved));
            job.recording_state.clear_if_save_pending();
        }
        Err(err) => tracing::error!("failed to trim replay clip: {err:#}"),
    }
}

#[cfg(test)]
fn save_and_trim(_job: SaveAndTrimJob) {
    panic!("tests must inject save handling instead of calling OBS");
}

fn spawn_save_and_trim(job: SaveAndTrimJob) {
    let spawned = std::thread::Builder::new().name("ge-replay-save".to_owned()).spawn(move || save_and_trim(job));
    if let Err(err) = spawned {
        tracing::error!("failed to spawn replay save thread: {err}");
    }
}

/// Trim the saved replay file down to the requested run window and write it
/// alongside the replay file with a descriptive name, returning the details of
/// the written clip.
fn trim_clip(req: TrimClipRequest<'_>) -> anyhow::Result<RecordingSaved> {
    let input = Path::new(req.replay_path);
    let duration = ffmpeg::duration_secs(input)?;
    // The file ends at ~the save moment. `start_before_save_secs` reaches back
    // to the detected start plus pre-run padding; `trim_tail_secs` removes any
    // extra delay beyond the requested post-run padding.
    let end = (duration - req.trim_tail_secs).clamp(0.0, duration);
    let start = (duration - req.start_before_save_secs).max(0.0).min(end);

    let failed = req.status.is_failed();
    let dir = output_dir(input, failed, req.options);
    ensure_output_directory(&dir)?;
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("replay");
    let ext = input.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    let relative_path = clip_relative_path(
        stem,
        req.status,
        req.completed_at,
        req.stats.as_ref(),
        req.options.clip_filename_template(),
    );
    let output = unique_output_path(&dir.join(append_extension(relative_path, ext)));
    if let Some(parent) = output.parent() {
        ensure_output_directory(parent)?;
    }

    tracing::info!(
        input = %input.display(),
        output = %output.display(),
        start,
        end = duration,
        trim_end = end,
        duration,
        failed,
        status = req.status.as_str(),
        "trimming replay clip",
    );
    let clip_metadata =
        clip_metadata(req.status, req.completed_at, req.stats.as_ref(), req.source_name, req.rom_language);
    ffmpeg::trim_with_metadata(input, &output, start, end, Some(&clip_metadata))?;
    tracing::info!(output = %output.display(), "saved trimmed clip");
    if failed
        && let Err(err) =
            prune_failed_clips(output.parent().unwrap_or_else(|| Path::new(".")), req.options.failed_run_limit)
    {
        tracing::warn!("failed to prune old failed clips: {err:#}");
    }

    Ok(RecordingSaved {
        save_id: req.save_id,
        path: output.to_string_lossy().into_owned(),
        replay_path: req.replay_path.to_owned(),
        // The clip spans [start, end]; clamping `start` above means this is the
        // buffer length when the run outran it, otherwise the configured window.
        duration_secs: end - start,
        failed,
        stats: req.stats,
    })
}

fn clip_metadata(
    status: RunStatus,
    completed_at: SystemTime,
    stats: Option<&LevelMatch>,
    source_name: &str,
    rom_language: &str,
) -> ffmpeg::ClipMetadata {
    let level_info = stats.and_then(|m| ge::level_info(m.mission, m.part));
    let time_seconds = stats.and_then(|m| m.times.map(|times| times.time.max(0)));

    ffmpeg::ClipMetadata {
        timestamp: format_iso_utc(completed_at),
        time: time_seconds.map(format_time),
        time_seconds,
        level: level_info.map(|info| info.name.to_owned()).unwrap_or_else(|| "unknown".to_owned()),
        level_number: level_info.map(|info| info.number),
        difficulty: stats.and_then(|m| ge::difficulty_name(m.difficulty)).map(str::to_owned),
        status: status.as_str().to_owned(),
        rom_language: rom_language.to_owned(),
        source_name: source_name.to_owned(),
        comment: format!("Created by The Golden Eye OBS plugin v{}", crate::PLUGIN_VERSION),
        plugin_version: crate::PLUGIN_VERSION.to_owned(),
    }
}

fn ensure_output_directory(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating output directory {}", dir.display()))?;

    let metadata = std::fs::metadata(dir).with_context(|| format!("checking output directory {}", dir.display()))?;
    if !metadata.is_dir() {
        anyhow::bail!("output path {} exists but is not a directory", dir.display());
    }

    Ok(())
}

fn output_dir(input: &Path, failed: bool, options: &RecordingOptions) -> PathBuf {
    if failed && let Some(path) = configured_dir(&options.failed_output_path) {
        return path;
    }
    if let Some(path) = configured_dir(&options.completed_output_path) {
        return path;
    }
    input.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
}

fn configured_dir(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(expand_home(trimmed))
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~"
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home);
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

fn unique_output_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("clip");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

    for i in 2.. {
        let file_name = if ext.is_empty() { format!("{stem} ({i})") } else { format!("{stem} ({i}).{ext}") };
        let candidate = parent.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("unbounded filename suffix search should always return")
}

fn prune_failed_clips(dir: &Path, keep: usize) -> anyhow::Result<()> {
    if keep == 0 {
        return Ok(());
    }

    let mut clips = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading failed clip directory {}", dir.display()))? {
        let Ok(entry) = entry else {
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        if let Some(metadata) = failed_clip_metadata(&path) {
            clips.push((metadata.timestamp, path));
        }
    }

    clips.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    for (_, path) in clips.into_iter().skip(keep) {
        tracing::info!(path = %path.display(), "pruning old failed clip");
        std::fs::remove_file(&path).with_context(|| format!("removing old failed clip {}", path.display()))?;
    }

    Ok(())
}

fn failed_clip_metadata(path: &Path) -> Option<ffmpeg::ClipMetadata> {
    match ffmpeg::read_clip_metadata(path) {
        Ok(Some(metadata)) if is_failed_clip_status(&metadata.status) => Some(metadata),
        Ok(_) => None,
        Err(err) => {
            tracing::debug!(path = %path.display(), "ignoring clip while pruning failed clips: {err:#}");
            None
        }
    }
}

fn is_failed_clip_status(status: &str) -> bool {
    RunStatus::from_str(status.trim()).is_some_and(RunStatus::is_failed)
}

/// Build an output path from the configured template and matched level info.
/// Collisions are handled by [`unique_output_path`], so terse templates remain
/// safe even when multiple runs render to the same relative path.
fn clip_relative_path(
    stem: &str,
    status: RunStatus,
    completed_at: SystemTime,
    stats: Option<&LevelMatch>,
    template: &str,
) -> PathBuf {
    let rendered = render_clip_template(template, stem, status, completed_at, stats);
    if let Some(path) = sanitize_relative_clip_path(&rendered) {
        path
    } else {
        sanitize_relative_clip_path(&render_clip_template(
            DEFAULT_CLIP_FILENAME_TEMPLATE,
            stem,
            status,
            completed_at,
            stats,
        ))
        .unwrap_or_else(|| PathBuf::from("clip"))
    }
}

fn render_clip_template(
    template: &str,
    stem: &str,
    status: RunStatus,
    completed_at: SystemTime,
    stats: Option<&LevelMatch>,
) -> String {
    let mission =
        stats.map(|m| if m.mission >= 0 { format!("{:02}", m.mission) } else { "??".to_owned() }).unwrap_or_default();
    let part = stats.map(|m| if m.part >= 0 { m.part.to_string() } else { "?".to_owned() }).unwrap_or_default();
    let difficulty = stats.and_then(|m| ge::difficulty_name(m.difficulty)).map(str::to_owned).unwrap_or_default();
    let level_info = stats.and_then(|m| ge::level_info(m.mission, m.part));
    let level = level_info.map(|info| info.name).unwrap_or("unknown");
    let level_number = level_info.map(|info| info.number.to_string()).unwrap_or_default();
    let time = stats.and_then(|m| m.times.map(|times| format_time(times.time))).unwrap_or_default();
    let timestamp = format_iso_utc(completed_at);
    let timestamp_local = format_iso_local(completed_at);

    template
        .replace("{obs_replay_name}", stem)
        .replace("{mission}", &mission)
        .replace("{part}", &part)
        .replace("{difficulty}", &difficulty)
        .replace("{level}", level)
        .replace("{levelNumber}", &level_number)
        .replace("{time}", &time)
        .replace("{status}", status.as_str())
        .replace("{timestamp}", &timestamp)
        .replace("{timestamp_local}", &timestamp_local)
}

fn format_time(seconds: i32) -> String {
    let seconds = seconds.max(0);
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn system_time_unix_seconds(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
        Err(err) => {
            let duration = err.duration();
            let seconds = i64::try_from(duration.as_secs()).unwrap_or(i64::MAX);
            if duration.subsec_nanos() == 0 { -seconds } else { -seconds - 1 }
        }
    }
}

fn div_floor(a: i64, b: i64) -> i64 {
    let quotient = a / b;
    let remainder = a % b;
    if remainder != 0 && ((remainder > 0) != (b > 0)) { quotient - 1 } else { quotient }
}

fn utc_from_unix_seconds(seconds: i64) -> (i64, i64, i64, i64, i64, i64) {
    let days = div_floor(seconds, 86_400);
    let seconds_of_day = seconds - days * 86_400;
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    // Howard Hinnant's civil-from-days conversion, using Unix day zero.
    let z = days + 719_468;
    let era = div_floor(z, 146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };

    (year, month, day, hour, minute, second)
}

fn format_iso_utc(time: SystemTime) -> String {
    let (year, month, day, hour, minute, second) = utc_from_unix_seconds(system_time_unix_seconds(time));
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(unix)]
fn format_iso_local(time: SystemTime) -> String {
    let seconds = system_time_unix_seconds(time);
    let time_t = seconds as libc::time_t;
    let mut local_tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    let local_tm = unsafe {
        if libc::localtime_r(&time_t, local_tm.as_mut_ptr()).is_null() {
            return format_iso_utc(time);
        }
        local_tm.assume_init()
    };
    let offset = local_tm.tm_gmtoff;
    let sign = if offset < 0 { '-' } else { '+' };
    let offset = offset.abs();
    let offset_hour = offset / 3_600;
    let offset_minute = (offset % 3_600) / 60;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{sign}{offset_hour:02}:{offset_minute:02}",
        local_tm.tm_year + 1900,
        local_tm.tm_mon + 1,
        local_tm.tm_mday,
        local_tm.tm_hour,
        local_tm.tm_min,
        local_tm.tm_sec,
    )
}

#[cfg(not(unix))]
fn format_iso_local(time: SystemTime) -> String {
    format_iso_utc(time)
}

fn append_extension(mut path: PathBuf, ext: &str) -> PathBuf {
    if ext.is_empty() {
        return path;
    }

    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("clip");
    path.set_file_name(format!("{file_name}.{ext}"));
    path
}

fn sanitize_relative_clip_path(path: &str) -> Option<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed.contains('\0') || trimmed.contains(wrong_platform_separator()) {
        return None;
    }

    let path = Path::new(trimmed);
    if path.is_absolute() {
        return None;
    }

    let mut sanitized = PathBuf::new();
    for component in trimmed.split(std::path::MAIN_SEPARATOR) {
        let component = sanitize_path_component(component);
        if component.is_empty() || component == "." || component == ".." {
            return None;
        }
        sanitized.push(component);
    }

    Some(sanitized)
}

fn wrong_platform_separator() -> char {
    if std::path::MAIN_SEPARATOR == '/' { '\\' } else { '/' }
}

fn sanitize_path_component(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect::<String>()
        .trim_matches(|c: char| c.is_whitespace() || c == '.')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    use super::*;
    use crate::ge::Times;

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(label: &str) -> Self {
            loop {
                let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
                let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
                let path =
                    std::env::temp_dir().join(format!("ge-recording-{label}-{}-{nanos}-{id}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => return TestDir { path },
                    Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(err) => panic!("failed to create test dir {}: {err}", path.display()),
                }
            }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn join(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_file(path: &Path) {
        fs::write(path, b"clip").unwrap();
    }

    fn test_recording(options: RecordingOptions) -> (RecordingState, tokio::sync::broadcast::Receiver<MonitorEvent>) {
        let (event_tx, event_rx) = tokio::sync::broadcast::channel(8);
        let (recording_tx, _recording_rx) = tokio::sync::watch::channel(None);
        let recording_state = RecordingStateStore::new(recording_tx);
        let recording =
            RecordingState::new(event_tx, recording_state, options, "N64 Capture".to_owned(), "en".to_owned());
        (recording, event_rx)
    }

    fn sample_clip() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/clips/sample_clip.mov")
    }

    fn test_clip_metadata(status: &str, timestamp: &str) -> ffmpeg::ClipMetadata {
        ffmpeg::ClipMetadata {
            timestamp: timestamp.to_owned(),
            time: Some("02:03".to_owned()),
            time_seconds: Some(123),
            level: "Surface 2".to_owned(),
            level_number: Some(8),
            difficulty: Some("00 Agent".to_owned()),
            status: status.to_owned(),
            rom_language: "en".to_owned(),
            source_name: "N64 Capture".to_owned(),
            comment: "Created by The Golden Eye OBS plugin test".to_owned(),
            plugin_version: "test".to_owned(),
        }
    }

    fn write_tagged_clip(path: &Path, status: &str, timestamp: &str) {
        let input = sample_clip();
        let full = ffmpeg::duration_secs(&input).expect("probe sample clip");
        let metadata = test_clip_metadata(status, timestamp);
        ffmpeg::trim_with_metadata(&input, path, 1.0, (full - 1.0).max(2.0), Some(&metadata))
            .expect("write tagged test clip");
    }

    fn match_with_time() -> LevelMatch {
        LevelMatch {
            screen: Screen::Stats,
            mission: 5,
            part: 1,
            difficulty: 2,
            detected_lang: None,
            times: Some(Times { time: 123, target_time: Some(100), best_time: Some(130) }),
            raw_times: vec![123, 100, 130],
            match_regions: Vec::new(),
            runtime_ms: 0.0,
        }
    }

    fn match_without_time() -> LevelMatch {
        LevelMatch {
            screen: Screen::Complete,
            mission: 1,
            part: 2,
            difficulty: 1,
            detected_lang: None,
            times: None,
            raw_times: Vec::new(),
            match_regions: Vec::new(),
            runtime_ms: 0.0,
        }
    }

    fn match_with_unreadable_header() -> LevelMatch {
        LevelMatch {
            screen: Screen::Stats,
            mission: -1,
            part: -1,
            difficulty: 99,
            detected_lang: None,
            times: Some(Times { time: -5, target_time: None, best_time: None }),
            raw_times: vec![-5],
            match_regions: Vec::new(),
            runtime_ms: 0.0,
        }
    }

    fn match_for_screen(screen: Screen) -> LevelMatch {
        let mut m = match_without_time();
        m.screen = screen;
        m
    }

    fn pending_save_event(events: &mut tokio::sync::broadcast::Receiver<MonitorEvent>) -> RecordingSavePending {
        let pending = events.try_recv().expect("pending save event");
        let MonitorEvent::RecordingSavePending(pending) = pending else {
            panic!("expected pending save event");
        };
        pending
    }

    fn assert_no_monitor_event(events: &mut tokio::sync::broadcast::Receiver<MonitorEvent>) {
        assert!(matches!(events.try_recv(), Err(tokio::sync::broadcast::error::TryRecvError::Empty)));
    }

    #[test]
    fn pre_run_padding_defaults_to_five_and_adds_match_buffer() {
        let default = RecordingOptions::default();
        assert_eq!(default.pre_run_padding_secs, DEFAULT_PRE_RUN_PADDING_SECS);
        assert_eq!(default.minimum_failed_run_length_secs, DEFAULT_MINIMUM_FAILED_RUN_LENGTH_SECS);
        assert_eq!(default.pre_run_padding_secs(), DEFAULT_PRE_RUN_PADDING_SECS + PRE_RUN_MATCH_BUFFER_SECS);

        let zero = RecordingOptions { pre_run_padding_secs: 0.0, ..RecordingOptions::default() };
        assert_eq!(zero.pre_run_padding_secs(), PRE_RUN_MATCH_BUFFER_SECS);

        let negative = RecordingOptions { pre_run_padding_secs: -2.0, ..RecordingOptions::default() };
        assert_eq!(negative.pre_run_padding_secs(), PRE_RUN_MATCH_BUFFER_SECS);
    }

    #[test]
    fn start_then_level_screen_cancels_active_session_without_save() {
        let (mut recording, mut events) = test_recording(RecordingOptions::default());
        let start = Instant::now();

        recording.on_frame(start, &match_for_screen(Screen::Start));
        recording.on_frame(start + Duration::from_secs(3), &match_for_screen(Screen::Unknown));
        recording.on_frame(start + Duration::from_secs(10), &match_for_screen(Screen::Levels));

        assert_eq!(recording.clip_start, None);
        assert_eq!(recording.status, None);
        assert!(recording.report.is_none());
        assert!(recording.pending.is_none());
        assert_eq!(recording.recording_state.current(), Some(RecordingStatus::Cancelled));
        assert_no_monitor_event(&mut events);
    }

    #[test]
    fn failed_report_then_stats_schedules_failed_save() {
        let (mut recording, mut events) = test_recording(RecordingOptions::default());
        let start = Instant::now();
        let failed_at = start + Duration::from_secs(8);
        let stats_at = start + Duration::from_secs(12);

        recording.on_frame(start, &match_for_screen(Screen::Start));
        recording.on_frame(start + Duration::from_secs(5), &match_for_screen(Screen::Unknown));
        recording.on_frame(failed_at, &match_for_screen(Screen::Failed));

        assert_eq!(recording.status, Some(RunStatus::Failed));
        assert_eq!(recording.report.as_ref().map(|m| m.screen), Some(Screen::Failed));
        assert_eq!(recording.recording_state.current(), Some(RecordingStatus::Failed));

        recording.on_frame(stats_at, &match_with_time());

        let pending = pending_save_event(&mut events);
        assert!(pending.failed);
        assert_eq!(pending.status, "failed");
        assert_eq!(pending.time_secs, Some(123));
        assert!((pending.estimated_duration_secs - 22.5).abs() < f64::EPSILON);
        assert_eq!(recording.clip_start, None);
        assert_eq!(recording.status, None);
        assert!(recording.report.is_none());
        assert_eq!(recording.recording_state.current(), Some(RecordingStatus::SavePending));

        let job = recording.take_pending_job(stats_at + Duration::from_secs(5)).expect("save job");
        assert_eq!(job.status, RunStatus::Failed);
        assert_eq!(job.stats.as_ref().map(|m| m.screen), Some(Screen::Stats));
        assert!((job.start_before_save_secs - 22.5).abs() < f64::EPSILON);
        assert_eq!(job.trim_tail_secs, 0.0);
    }

    #[test]
    fn complete_report_then_stats_schedules_completed_save() {
        let (mut recording, mut events) = test_recording(RecordingOptions::default());
        let start = Instant::now();
        let complete_at = start + Duration::from_secs(20);
        let stats_at = start + Duration::from_secs(22);

        recording.on_frame(start, &match_for_screen(Screen::Start));
        recording.on_frame(start + Duration::from_secs(5), &match_for_screen(Screen::Unknown));
        recording.on_frame(complete_at, &match_for_screen(Screen::Complete));

        assert_eq!(recording.status, Some(RunStatus::Complete));
        assert_eq!(recording.report.as_ref().map(|m| m.screen), Some(Screen::Complete));
        assert_eq!(recording.recording_state.current(), Some(RecordingStatus::Complete));

        recording.on_frame(stats_at, &match_with_time());

        let pending = pending_save_event(&mut events);
        assert!(!pending.failed);
        assert_eq!(pending.status, "complete");
        assert_eq!(pending.time_secs, Some(123));
        assert!((pending.estimated_duration_secs - 32.5).abs() < f64::EPSILON);
        assert_eq!(recording.clip_start, None);
        assert_eq!(recording.status, None);
        assert!(recording.report.is_none());
        assert_eq!(recording.recording_state.current(), Some(RecordingStatus::SavePending));

        let job = recording.take_pending_job(stats_at + Duration::from_secs(5)).expect("save job");
        assert_eq!(job.status, RunStatus::Complete);
        assert_eq!(job.stats.as_ref().map(|m| m.screen), Some(Screen::Stats));
    }

    #[test]
    fn complete_report_then_level_screen_saves_as_stats_skipped() {
        let (mut recording, mut events) = test_recording(RecordingOptions::default());
        let start = Instant::now();
        let complete_at = start + Duration::from_secs(20);
        let levels_at = start + Duration::from_secs(24);

        recording.on_frame(start, &match_for_screen(Screen::Start));
        recording.on_frame(complete_at, &match_for_screen(Screen::Complete));
        recording.on_frame(levels_at, &match_for_screen(Screen::Levels));

        let pending = pending_save_event(&mut events);
        assert!(!pending.failed);
        assert_eq!(pending.status, "complete");
        assert_eq!(pending.time_secs, None);
        assert_eq!(pending.stats.as_ref().map(|m| m.screen), Some(Screen::Complete));
        assert_eq!(recording.recording_state.current(), Some(RecordingStatus::StatsSkipped));

        let job = recording.take_pending_job(levels_at + Duration::from_secs(5)).expect("save job");
        assert_eq!(job.status, RunStatus::Complete);
        assert_eq!(job.stats.as_ref().map(|m| m.screen), Some(Screen::Complete));
    }

    #[test]
    fn failed_report_then_level_screen_schedules_save_without_stats_skipped() {
        let (mut recording, mut events) = test_recording(RecordingOptions::default());
        let start = Instant::now();
        let failed_at = start + Duration::from_secs(20);
        let levels_at = start + Duration::from_secs(24);

        recording.on_frame(start, &match_for_screen(Screen::Start));
        recording.on_frame(failed_at, &match_for_screen(Screen::Failed));
        recording.on_frame(levels_at, &match_for_screen(Screen::Levels));

        let pending = pending_save_event(&mut events);
        assert!(pending.failed);
        assert_eq!(pending.status, "failed");
        assert_eq!(pending.time_secs, None);
        assert_eq!(pending.stats.as_ref().map(|m| m.screen), Some(Screen::Failed));
        assert_eq!(recording.recording_state.current(), Some(RecordingStatus::SavePending));

        let job = recording.take_pending_job(levels_at + Duration::from_secs(5)).expect("save job");
        assert_eq!(job.status, RunStatus::Failed);
        assert_eq!(job.stats.as_ref().map(|m| m.screen), Some(Screen::Failed));
    }

    #[test]
    fn failed_run_discarded_when_failed_saves_are_disabled() {
        let options = RecordingOptions { save_failed_runs: false, ..RecordingOptions::default() };
        let (mut recording, mut events) = test_recording(options);
        let start = Instant::now();

        recording.on_frame(start, &match_for_screen(Screen::Start));
        recording.on_frame(start + Duration::from_secs(10), &match_for_screen(Screen::Failed));
        recording.on_frame(start + Duration::from_secs(12), &match_with_time());

        assert_eq!(recording.clip_start, None);
        assert_eq!(recording.status, None);
        assert!(recording.report.is_none());
        assert!(recording.pending.is_none());
        assert_eq!(recording.recording_state.current(), Some(RecordingStatus::FailedDiscarded));
        assert_no_monitor_event(&mut events);
    }

    #[test]
    fn complete_report_after_failure_clears_failure_and_saves_completed_stats() {
        let (mut recording, mut events) = test_recording(RecordingOptions::default());
        let start = Instant::now();
        let stats_at = start + Duration::from_secs(15);

        recording.on_frame(start, &match_for_screen(Screen::Start));
        recording.on_frame(start + Duration::from_secs(8), &match_for_screen(Screen::Failed));
        assert_eq!(recording.recording_state.current(), Some(RecordingStatus::Failed));

        recording.on_frame(start + Duration::from_secs(10), &match_for_screen(Screen::Complete));
        assert_eq!(recording.status, Some(RunStatus::Complete));
        assert_eq!(recording.recording_state.current(), Some(RecordingStatus::Complete));

        recording.on_frame(stats_at, &match_with_time());

        let pending = pending_save_event(&mut events);
        assert!(!pending.failed);
        assert_eq!(pending.status, "complete");
        assert_eq!(pending.time_secs, Some(123));

        let job = recording.take_pending_job(stats_at + Duration::from_secs(5)).expect("save job");
        assert_eq!(job.status, RunStatus::Complete);
        assert_eq!(job.stats.as_ref().map(|m| m.screen), Some(Screen::Stats));
    }

    #[test]
    fn terminal_screens_without_active_session_are_ignored() {
        let (mut recording, mut events) = test_recording(RecordingOptions::default());
        let now = Instant::now();

        for screen in [Screen::Failed, Screen::Abort, Screen::Kia, Screen::Complete, Screen::Stats, Screen::Levels] {
            let m = if screen == Screen::Stats { match_with_time() } else { match_for_screen(screen) };
            recording.on_frame(now, &m);
            assert_eq!(recording.clip_start, None);
            assert_eq!(recording.status, None);
            assert!(recording.report.is_none());
            assert!(recording.pending.is_none());
            assert_eq!(recording.recording_state.current(), None);
            assert_no_monitor_event(&mut events);
        }
    }

    #[test]
    fn duplicate_start_frames_do_not_reset_the_session_anchor() {
        let (mut recording, mut events) = test_recording(RecordingOptions::default());
        let start = Instant::now();
        let duplicate_start = start + Duration::from_secs(10);
        let stats_at = start + Duration::from_secs(20);

        recording.on_frame(start, &match_for_screen(Screen::Start));
        recording.on_frame(duplicate_start, &match_for_screen(Screen::Start));
        assert_eq!(recording.clip_start, Some(start));

        recording.on_frame(stats_at, &match_with_time());

        let pending = pending_save_event(&mut events);
        assert_eq!(pending.status, "complete");
        assert!((pending.estimated_duration_secs - 30.5).abs() < f64::EPSILON);

        let job = recording.take_pending_job(stats_at + Duration::from_secs(5)).expect("save job");
        assert_eq!(job.status, RunStatus::Complete);
        assert!((job.start_before_save_secs - 30.5).abs() < f64::EPSILON);
    }

    #[test]
    fn failure_screen_variants_emit_distinct_statuses_and_save_statuses() {
        for (screen, recording_status, run_status, pending_status) in [
            (Screen::Failed, RecordingStatus::Failed, RunStatus::Failed, "failed"),
            (Screen::Abort, RecordingStatus::Aborted, RunStatus::Abort, "abort"),
            (Screen::Kia, RecordingStatus::Kia, RunStatus::Kia, "kia"),
        ] {
            let (mut recording, mut events) = test_recording(RecordingOptions::default());
            let start = Instant::now();
            let stats_at = start + Duration::from_secs(12);

            recording.on_frame(start, &match_for_screen(Screen::Start));
            recording.on_frame(start + Duration::from_secs(10), &match_for_screen(screen));

            assert_eq!(recording.status, Some(run_status));
            assert_eq!(recording.report.as_ref().map(|m| m.screen), Some(screen));
            assert_eq!(recording.recording_state.current(), Some(recording_status));

            recording.on_frame(stats_at, &match_with_time());

            let pending = pending_save_event(&mut events);
            assert!(pending.failed);
            assert_eq!(pending.status, pending_status);

            let job = recording.take_pending_job(stats_at + Duration::from_secs(5)).expect("save job");
            assert_eq!(job.status, run_status);
        }
    }

    #[test]
    fn output_dir_prefers_failed_then_completed_then_replay_parent() {
        let dir = TestDir::new("output-dir");
        let input = dir.join("replay.mov");
        let completed = dir.join("completed");
        let failed = dir.join("failed");

        let mut options = RecordingOptions {
            completed_output_path: completed.to_string_lossy().into_owned(),
            failed_output_path: failed.to_string_lossy().into_owned(),
            ..RecordingOptions::default()
        };

        assert_eq!(output_dir(&input, false, &options), completed);
        assert_eq!(output_dir(&input, true, &options), failed);

        options.failed_output_path.clear();
        assert_eq!(output_dir(&input, true, &options), completed);

        options.completed_output_path.clear();
        assert_eq!(output_dir(&input, true, &options), dir.path());
    }

    #[test]
    fn ensure_output_directory_creates_nested_missing_directory() {
        let dir = TestDir::new("ensure-output");
        let output = dir.join("completed/deeply/nested");

        assert!(!output.exists());
        ensure_output_directory(&output).unwrap();

        assert!(output.is_dir());
    }

    #[test]
    fn ensure_output_directory_rejects_existing_file() {
        let dir = TestDir::new("ensure-output-file");
        let output = dir.join("completed");
        write_file(&output);

        let err = ensure_output_directory(&output).unwrap_err();

        assert!(
            err.to_string().contains("creating output directory")
                || err.to_string().contains("exists but is not a directory"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn shutdown_before_pending_save_fires_waits_and_preserves_save_job() {
        let options =
            RecordingOptions { pre_run_padding_secs: 1.0, post_run_padding_secs: 5.0, ..RecordingOptions::default() };
        let (mut recording, mut events) = test_recording(options);
        let start = Instant::now();
        let stats_at = start + Duration::from_secs(10);

        assert!(recording.schedule_save(stats_at, start, Some(match_with_time())));

        let pending = events.try_recv().expect("pending save event");
        let MonitorEvent::RecordingSavePending(pending) = pending else {
            panic!("expected pending save event");
        };
        assert_eq!(pending.save_id, 1);
        assert_eq!(pending.save_in_secs, 5.0);
        assert_eq!(pending.level, "Surface 2");
        assert_eq!(pending.time_secs, Some(123));

        let slept = RefCell::new(None);
        let saved_job = RefCell::new(None);
        recording.flush_pending_on_shutdown_with(
            stats_at + Duration::from_secs(2),
            |duration| *slept.borrow_mut() = Some(duration),
            |job| *saved_job.borrow_mut() = Some(job),
        );

        assert_eq!(*slept.borrow(), Some(Duration::from_secs(3)));
        let job = saved_job.borrow_mut().take().expect("save job");
        assert_eq!(job.save_id, 1);
        assert_eq!(job.status, RunStatus::Complete);
        assert!(job.completed_at <= SystemTime::now());
        assert_eq!(job.stats.as_ref().and_then(|m| m.times).map(|times| times.time), Some(123));
        assert_eq!(job.options.pre_run_padding_secs, 1.0);
        assert_eq!(job.options.post_run_padding_secs, 5.0);
        assert_eq!(job.source_name, "N64 Capture");
        assert_eq!(job.rom_language, "en");
        assert_eq!(job.event_tx.receiver_count(), 1);
        assert_eq!(job.recording_state.current(), None);
        assert!((job.start_before_save_secs - 16.5).abs() < f64::EPSILON);
        assert_eq!(job.trim_tail_secs, 0.0);
        assert!(recording.pending.is_none());
    }

    #[test]
    fn shutdown_after_pending_save_fire_time_flushes_without_waiting() {
        let options =
            RecordingOptions { pre_run_padding_secs: 1.0, post_run_padding_secs: 5.0, ..RecordingOptions::default() };
        let (mut recording, _events) = test_recording(options);
        let start = Instant::now();
        let stats_at = start + Duration::from_secs(10);

        assert!(recording.schedule_save(stats_at, start, Some(match_with_time())));

        let slept = RefCell::new(None);
        let saved_job = RefCell::new(None);
        recording.flush_pending_on_shutdown_with(
            stats_at + Duration::from_secs(7),
            |duration| *slept.borrow_mut() = Some(duration),
            |job| *saved_job.borrow_mut() = Some(job),
        );

        assert_eq!(*slept.borrow(), None);
        let job = saved_job.borrow_mut().take().expect("save job");
        assert_eq!(job.save_id, 1);
        assert!((job.start_before_save_secs - 18.5).abs() < f64::EPSILON);
        assert_eq!(job.trim_tail_secs, 2.0);
        assert!(recording.pending.is_none());
    }

    #[test]
    fn failed_run_without_stats_shorter_than_minimum_length_is_not_scheduled() {
        let options = RecordingOptions { minimum_failed_run_length_secs: 20.0, ..RecordingOptions::default() };
        let (mut recording, mut events) = test_recording(options);
        let start = Instant::now();
        let failed_at = start + Duration::from_secs(19);

        recording.status = Some(RunStatus::Failed);
        assert!(!recording.schedule_save(failed_at, start, Some(match_without_time())));

        assert!(recording.pending.is_none());
        assert!(matches!(events.try_recv(), Err(tokio::sync::broadcast::error::TryRecvError::Empty)));
    }

    #[test]
    fn failed_run_without_stats_at_or_above_minimum_length_is_scheduled() {
        let options = RecordingOptions { minimum_failed_run_length_secs: 20.0, ..RecordingOptions::default() };
        let (mut recording, mut events) = test_recording(options);
        let start = Instant::now();
        let failed_at = start + Duration::from_secs(20);

        recording.status = Some(RunStatus::Failed);
        assert!(recording.schedule_save(failed_at, start, Some(match_without_time())));

        let pending = events.try_recv().expect("pending save event");
        let MonitorEvent::RecordingSavePending(pending) = pending else {
            panic!("expected pending save event");
        };
        assert!(pending.failed);
        assert_eq!(pending.status, "failed");
        assert!((pending.estimated_duration_secs - 30.5).abs() < f64::EPSILON);
        recording.pending = None;
    }

    #[test]
    fn failed_run_minimum_length_uses_stats_time_when_present() {
        let options = RecordingOptions { minimum_failed_run_length_secs: 20.0, ..RecordingOptions::default() };
        let (mut recording, mut events) = test_recording(options);
        let start = Instant::now();
        let failed_at = start + Duration::from_secs(25);
        let mut stats = match_with_time();
        stats.times = Some(Times { time: 19, target_time: None, best_time: None });

        recording.status = Some(RunStatus::Failed);
        assert!(!recording.schedule_save(failed_at, start, Some(stats)));

        assert!(recording.pending.is_none());
        assert!(matches!(events.try_recv(), Err(tokio::sync::broadcast::error::TryRecvError::Empty)));
    }

    #[test]
    fn failed_run_minimum_length_accepts_stats_time_at_threshold() {
        let options = RecordingOptions { minimum_failed_run_length_secs: 20.0, ..RecordingOptions::default() };
        let (mut recording, mut events) = test_recording(options);
        let start = Instant::now();
        let failed_at = start + Duration::from_secs(10);
        let mut stats = match_with_time();
        stats.times = Some(Times { time: 20, target_time: None, best_time: None });

        recording.status = Some(RunStatus::Failed);
        assert!(recording.schedule_save(failed_at, start, Some(stats)));

        let pending = events.try_recv().expect("pending save event");
        let MonitorEvent::RecordingSavePending(pending) = pending else {
            panic!("expected pending save event");
        };
        assert!(pending.failed);
        assert_eq!(pending.status, "failed");
        assert_eq!(pending.time_secs, Some(20));
        assert!((pending.estimated_duration_secs - 20.5).abs() < f64::EPSILON);
        recording.pending = None;
    }

    #[test]
    fn zero_minimum_failed_run_length_saves_all_failed_runs() {
        let options = RecordingOptions { minimum_failed_run_length_secs: 0.0, ..RecordingOptions::default() };
        let (mut recording, mut events) = test_recording(options);
        let start = Instant::now();

        recording.status = Some(RunStatus::Failed);
        assert!(recording.schedule_save(start, start, Some(match_without_time())));

        let pending = events.try_recv().expect("pending save event");
        let MonitorEvent::RecordingSavePending(pending) = pending else {
            panic!("expected pending save event");
        };
        assert!(pending.failed);
        recording.pending = None;
    }

    #[test]
    fn trim_clip_creates_missing_completed_and_failed_output_directories() {
        let dir = TestDir::new("trim-missing-output");
        let replay_path = sample_clip();
        let replay = replay_path.to_string_lossy();
        let completed = dir.join("completed/deeply/nested");
        let failed = dir.join("failed/deeply/nested");
        let options = RecordingOptions {
            completed_output_path: completed.to_string_lossy().into_owned(),
            failed_output_path: failed.to_string_lossy().into_owned(),
            clip_filename_template: "{status}-{obs_replay_name}".to_owned(),
            ..RecordingOptions::default()
        };

        let complete_saved = trim_clip(TrimClipRequest {
            save_id: 1,
            replay_path: &replay,
            start_before_save_secs: 1.0,
            trim_tail_secs: 0.0,
            status: RunStatus::Complete,
            completed_at: UNIX_EPOCH,
            stats: Some(match_with_time()),
            options: &options,
            source_name: "N64 Capture",
            rom_language: "en",
        })
        .expect("trim completed clip");

        let failed_saved = trim_clip(TrimClipRequest {
            save_id: 2,
            replay_path: &replay,
            start_before_save_secs: 1.0,
            trim_tail_secs: 0.0,
            status: RunStatus::Failed,
            completed_at: UNIX_EPOCH + Duration::from_secs(1),
            stats: Some(match_with_time()),
            options: &options,
            source_name: "N64 Capture",
            rom_language: "en",
        })
        .expect("trim failed clip");

        let complete_path = PathBuf::from(&complete_saved.path);
        let failed_path = PathBuf::from(&failed_saved.path);
        assert!(completed.is_dir());
        assert!(failed.is_dir());
        assert!(complete_path.starts_with(&completed), "{}", complete_path.display());
        assert!(failed_path.starts_with(&failed), "{}", failed_path.display());
        assert!(complete_path.is_file());
        assert!(failed_path.is_file());
        assert!(!complete_saved.failed);
        assert!(failed_saved.failed);
    }

    #[test]
    fn unique_output_path_chooses_first_available_numeric_suffix() {
        let dir = TestDir::new("unique-output");
        let base = dir.join("clip.mp4");
        let second = dir.join("clip (2).mp4");
        write_file(&base);
        write_file(&second);

        let third = dir.join("clip (3).mp4");
        assert_eq!(unique_output_path(&base), third);
        assert!(!third.exists());

        let no_ext = dir.join("clip");
        write_file(&no_ext);
        assert_eq!(unique_output_path(&no_ext), dir.join("clip (2)"));
    }

    #[test]
    fn render_clip_template_replaces_all_supported_tokens() {
        let m = match_with_time();
        let completed_at = UNIX_EPOCH + Duration::from_secs(1_700_000_000);

        let rendered = render_clip_template(
            "{obs_replay_name}|{mission}|{part}|{levelNumber}|{level}|{time}|{difficulty}|{status}|{timestamp}|{timestamp_local}",
            "obs replay",
            RunStatus::Complete,
            completed_at,
            Some(&m),
        );

        assert_eq!(
            rendered,
            format!(
                "obs replay|05|1|8|Surface 2|02:03|00 Agent|complete|2023-11-14T22:13:20Z|{}",
                format_iso_local(completed_at),
            ),
        );
    }

    #[test]
    fn render_clip_template_uses_empty_fields_without_stats() {
        let rendered = render_clip_template(
            "{level}|{mission}|{part}|{levelNumber}|{time}|{difficulty}|{status}|{obs_replay_name}",
            "replay",
            RunStatus::Failed,
            UNIX_EPOCH,
            None,
        );

        assert_eq!(rendered, "unknown||||||failed|replay");
    }

    #[test]
    fn render_clip_template_omits_time_when_report_has_no_stats_row() {
        let m = match_without_time();

        let rendered = render_clip_template(
            "{mission}-{part}-{levelNumber}-{level}-{time}-{difficulty}-{status}",
            "replay",
            RunStatus::Abort,
            UNIX_EPOCH,
            Some(&m),
        );

        assert_eq!(rendered, "01-2-2-Facility--Secret Agent-abort");
    }

    #[test]
    fn render_clip_template_marks_unreadable_header_parts() {
        let m = match_with_unreadable_header();

        let rendered = render_clip_template(
            "{mission}|{part}|{levelNumber}|{level}|{time}|{difficulty}|{status}",
            "replay",
            RunStatus::Kia,
            UNIX_EPOCH,
            Some(&m),
        );

        assert_eq!(rendered, "??|?||unknown|00:00||kia");
    }

    #[test]
    fn render_clip_template_leaves_unknown_tokens_and_unsanitized_text() {
        let m = match_with_time();

        let rendered = render_clip_template(
            "{obs_replay_name}/{not_a_token}/{level}:{status}",
            "OBS/Replay:01",
            RunStatus::Complete,
            UNIX_EPOCH,
            Some(&m),
        );

        assert_eq!(rendered, "OBS/Replay:01/{not_a_token}/Surface 2:complete");
    }

    #[test]
    fn clip_template_renders_and_sanitizes_relative_paths() {
        let m = match_with_time();

        let rendered = render_clip_template(
            "{obs_replay_name}-{mission}-{part}-{levelNumber}-{level}-{time}-{difficulty}-{status}-{timestamp}",
            "obs replay",
            RunStatus::Abort,
            UNIX_EPOCH,
            Some(&m),
        );
        assert_eq!(rendered, "obs replay-05-1-8-Surface 2-02:03-00 Agent-abort-1970-01-01T00:00:00Z");

        let path = clip_relative_path(
            "OBS/Replay:01",
            RunStatus::Kia,
            UNIX_EPOCH,
            Some(&m),
            "{level}/{difficulty}/{time}?{status}",
        );
        let name = path.file_name().and_then(|name| name.to_str()).unwrap();
        for forbidden in ['/', '\\', ':', '*', '?', '"', '<', '>', '|'] {
            assert!(!name.contains(forbidden), "{name:?} still contains {forbidden:?}");
        }
        assert_eq!(path.parent().unwrap(), Path::new("Surface 2").join("00 Agent"));
        assert!(name.contains("02-03"));
        assert!(name.ends_with("-kia"));

        assert_eq!(
            clip_relative_path("replay", RunStatus::Complete, UNIX_EPOCH, None, "..."),
            PathBuf::from("unknown -  -  - complete"),
        );
    }

    #[test]
    fn clip_template_rejects_traversal_and_wrong_platform_separator() {
        let m = match_with_time();

        assert_eq!(
            clip_relative_path("replay", RunStatus::Complete, UNIX_EPOCH, Some(&m), "../{level}"),
            PathBuf::from("Surface 2 - 02-03 - 00 Agent - complete"),
        );
        assert_eq!(
            clip_relative_path("replay", RunStatus::Complete, UNIX_EPOCH, Some(&m), "{level}/../{time}"),
            PathBuf::from("Surface 2 - 02-03 - 00 Agent - complete"),
        );
        assert_eq!(
            clip_relative_path(
                "replay",
                RunStatus::Complete,
                UNIX_EPOCH,
                Some(&m),
                if std::path::MAIN_SEPARATOR == '/' { "{level}\\{time}" } else { "{level}/{time}" },
            ),
            PathBuf::from("Surface 2 - 02-03 - 00 Agent - complete"),
        );
    }

    #[test]
    fn prune_failed_clips_keep_zero_is_unlimited_and_deletes_nothing() {
        let dir = TestDir::new("prune-unlimited");
        let old = dir.join("obs - clip - old - failed.mp4");
        let saved = dir.join("obs - clip - saved - failed.mp4");
        write_file(&old);
        write_file(&saved);

        prune_failed_clips(dir.path(), 0).unwrap();

        assert!(old.exists());
        assert!(saved.exists());
        assert!(!dir.join(".the-golden-eye-failed-clips.json").exists());
    }

    #[test]
    fn prune_failed_clips_uses_metadata_status_and_timestamp_only() {
        let dir = TestDir::new("prune-metadata");
        let old_failed = dir.join("custom old.mov");
        let newer_abort = dir.join("not named failed.mov");
        let saved_kia = dir.join("saved with custom name.mov");
        let complete_named_failed = dir.join("complete but filename says failed.mov");
        let unreadable_named_failed = dir.join("unreadable filename says failed.mov");

        write_tagged_clip(&old_failed, "failed", "2026-01-01T00:00:00Z");
        write_tagged_clip(&newer_abort, "abort", "2026-01-03T00:00:00Z");
        write_tagged_clip(&saved_kia, "kia", "2026-01-02T00:00:00Z");
        write_tagged_clip(&complete_named_failed, "complete", "2026-01-04T00:00:00Z");
        write_file(&unreadable_named_failed);

        prune_failed_clips(dir.path(), 2).unwrap();

        assert!(!old_failed.exists(), "oldest metadata-tagged failed clip should be pruned");
        assert!(newer_abort.exists(), "newest metadata-tagged failed clip should be kept");
        assert!(saved_kia.exists(), "second-newest metadata-tagged failed clip should be kept");
        assert!(complete_named_failed.exists(), "complete clips must not be pruned based on filename");
        assert!(unreadable_named_failed.exists(), "unreadable files must be ignored");
        assert!(!dir.join(".the-golden-eye-failed-clips.json").exists());
    }
}
