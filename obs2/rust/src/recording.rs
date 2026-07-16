//! Replay-buffer driven recording. We keep OBS's replay buffer running for the whole
//! session and save/trim (via [`crate::ffmpeg`]) a window out of it per run, rather
//! than start/stop per run. Padding is anchored to the save moment (file ends at ~now).

use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, c_char};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::cv::{LevelMatch, Screen};
use crate::http::{
    FailedRunNotSavedReason,
    MonitorEvent,
    RecordingSavePending,
    RecordingSaved,
    RecordingStateStore,
    RecordingStatus,
};
use crate::{ffmpeg, ge};

/// Default filename template for trimmed clips. Uses platform-native separators
/// and falls back through the unique-name suffixer when multiple runs render alike.
#[cfg(windows)]
pub const DEFAULT_CLIP_FILENAME_TEMPLATE: &str = "{level}\\{difficulty}\\{time} - {timestamp_local}";
#[cfg(not(windows))]
pub const DEFAULT_CLIP_FILENAME_TEMPLATE: &str = "{level}/{difficulty}/{time} - {timestamp_local}";
pub const DEFAULT_PRE_RUN_PADDING_SECS: f64 = 5.0;
pub const DEFAULT_POST_RUN_PADDING_SECS: f64 = 5.0;
pub const DEFAULT_FAILED_RUN_LIMIT: usize = 10;
pub const DEFAULT_MINIMUM_FAILED_RUN_LENGTH_SECS: f64 = 20.0;
/// Internal safety margin added to both the pre- and post-run padding, on top of
/// the user's configured values and hidden from them, so a single-frame timing
/// window can't drop the level-start briefing or stats overlay (e.g. padding 0).
const MATCH_PADDING_BUFFER_SECS: f64 = 0.5;

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
            failed_run_limit: DEFAULT_FAILED_RUN_LIMIT,
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
        Self::non_negative_secs(self.pre_run_padding_secs, 0.0) + MATCH_PADDING_BUFFER_SECS
    }

    fn post_run_padding_secs(&self) -> f64 {
        Self::non_negative_secs(self.post_run_padding_secs, DEFAULT_POST_RUN_PADDING_SECS) + MATCH_PADDING_BUFFER_SECS
    }

    fn minimum_failed_run_length_secs(&self) -> f64 {
        Self::non_negative_secs(self.minimum_failed_run_length_secs, 0.0)
    }

    fn save_delay(&self) -> Duration {
        Duration::from_secs_f64(self.post_run_padding_secs())
    }
}

/// The latest replay-saved event, published by the OBS frontend callback and
/// awaited by the save thread.
struct ReplaySaved {
    /// Ticks per event so a waiter can tell a fresh event from a stale one.
    generation: u64,
    /// The file OBS just wrote, or `None` if it reported none.
    last_path: Option<String>,
    /// Plugin-initiated saves still awaiting their event; when zero, a saved
    /// event is the user's own manual save, which we leave untouched.
    pending_requests: u32,
}

static REPLAY_SAVED: Mutex<ReplaySaved> =
    Mutex::new(ReplaySaved { generation: 0, last_path: None, pending_requests: 0 });
static REPLAY_SAVED_CV: Condvar = Condvar::new();

/// Serializes plugin-initiated saves so at most one is outstanding: OBS's saved
/// event has no identity, so two in flight could both wake on it and trim the same
/// file. Only the request + wait need it (one at a time), not the subsequent trim.
#[cfg(not(test))]
static REPLAY_SAVE_SERIALIZE: Mutex<()> = Mutex::new(());

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
    // No plugin save is outstanding, so this is the user saving the buffer
    // themselves. Leave it alone: don't record it as ours, so no save thread
    // ever trims or deletes a file the user asked OBS to keep.
    if guard.pending_requests == 0 {
        tracing::debug!(?path, "ignoring user-initiated replay buffer save");
        return;
    }
    guard.pending_requests -= 1;
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

/// Register a pending plugin save and return the generation to wait past.
/// Incrementing before the save call (so an immediate event still counts as ours)
/// lets [`on_replay_saved`] tell our saves from the user's manual ones.
#[cfg(not(test))]
fn begin_replay_save_request() -> u64 {
    let mut guard = REPLAY_SAVED.lock().unwrap_or_else(|p| p.into_inner());
    guard.pending_requests = guard.pending_requests.saturating_add(1);
    guard.generation
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
            // Our event never arrived; release the request so a later user save
            // isn't mistaken for it. `on_replay_saved` holds the same lock, so a
            // just-claimed event would have advanced `generation` and exited above.
            guard.pending_requests = guard.pending_requests.saturating_sub(1);
            return None;
        }
        let (next, res) = REPLAY_SAVED_CV.wait_timeout(guard, timeout - elapsed).unwrap_or_else(|p| p.into_inner());
        guard = next;
        if res.timed_out() && guard.generation == since {
            guard.pending_requests = guard.pending_requests.saturating_sub(1);
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

/// A scheduled save that *will* happen, captured in full when the stats screen is
/// seen. Decoupled from the active-run state: once scheduled it owns all it needs,
/// so backing out or starting another run can't drop it -- it fires on its own timer.
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
    /// The stats-screen match, kept for naming the output clip. Its `times` are
    /// overwritten with the per-field vote winners as stats frames arrive.
    stats: Option<LevelMatch>,
    /// Independent per-field vote over the stats times, so a look-alike-digit
    /// misread on one field (often the dimmer best-time row) can't corrupt the
    /// others. Empty for saves not scheduled off the stats screen.
    time_vote: FieldVote,
    target_vote: FieldVote,
    best_vote: FieldVote,
    /// Set once the screen leaves stats: the vote is locked so a later run's stats
    /// screen (within the padding window) can't fold into this save.
    stats_vote_closed: bool,
    /// Whether a "saving recording" notification is currently shown for this save.
    /// Tracked so it is only sent once the run is savable, and reliably cleared if
    /// the save is later discarded.
    notified: bool,
    /// The phase-store generation of this save's own `SavePending`/`StatsSkipped`
    /// transition, if it emitted one. Its completion/discard clears exactly that
    /// transition, not a quick-restarted run's identical-looking phase.
    phase_generation: Option<u64>,
}

/// Whether the pending save currently passes the save criteria. Failed runs
/// shorter than the configured minimum are dropped; everything else is savable.
/// Uses the stats-screen time when present, otherwise the wall-clock run length.
fn pending_is_savable(options: &RecordingOptions, pending: &PendingSave) -> bool {
    if !pending.status.is_failed() {
        return true;
    }
    let length_secs = failed_run_length_secs(pending.finish_at, pending.clip_start, pending.stats.as_ref());
    length_secs >= options.minimum_failed_run_length_secs()
}

/// Frame-count vote for one stats-time field. The most-seen value wins, ties
/// resolving to the newest reading, so a brief first-frame misread is outvoted
/// by the stable one.
#[derive(Default)]
struct FieldVote {
    counts: HashMap<Option<i32>, u32>,
    best_count: u32,
    winner: Option<i32>,
}

impl FieldVote {
    /// Records one reading; returns whether the winning value changed.
    fn record(&mut self, value: Option<i32>) -> bool {
        let count = {
            let c = self.counts.entry(value).or_insert(0);
            *c += 1;
            *c
        };
        if count < self.best_count {
            return false;
        }
        let changed = self.winner != value;
        self.best_count = count;
        self.winner = value;
        changed
    }
}

/// Record one stats reading, voting each time field independently, and refresh
/// the stored match with the per-field winners. Returns whether any voted field
/// changed (so the pending notification can be reissued).
fn record_stats_vote(pending: &mut PendingSave, m: &LevelMatch) -> bool {
    let times = m.times;
    let mut changed = pending.time_vote.record(times.map(|t| t.time));
    changed |= pending.target_vote.record(times.and_then(|t| t.target_time));
    changed |= pending.best_vote.record(times.and_then(|t| t.best_time));
    // Keep the newest frame as the naming/level source, then overwrite its times
    // with the voted winners so downstream naming/metadata use the stable values.
    let mut stats = m.clone();
    stats.times = pending.time_vote.winner.map(|time| crate::ge::Times {
        time,
        target_time: pending.target_vote.winner,
        best_time: pending.best_vote.winner,
    });
    pending.stats = Some(stats);
    changed
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

/// Build the notification for a pending save, reading `save_in_secs` as the time
/// remaining until it fires. Used for the initial event and for the update sent
/// when the voted time is refined.
fn save_pending_event(pending: &PendingSave, options: &RecordingOptions, now: Instant) -> RecordingSavePending {
    let run_length_secs = pending.finish_at.saturating_duration_since(pending.clip_start).as_secs_f64();
    let estimated_duration_secs = run_length_secs + options.pre_run_padding_secs() + options.post_run_padding_secs();
    recording_save_pending_event(
        pending.save_id,
        pending.fire_at.saturating_duration_since(now),
        estimated_duration_secs,
        pending.status,
        pending.stats.as_ref(),
    )
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
    /// The post-mission report screen (Complete/Failed/Abort/KIA) match, or `None`
    /// if not reached. Presence means the run finished (so backing out still saves);
    /// absence means abandoned. Also names the clip when the stats screen is skipped.
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

    /// Publish a recorder state transition to the backend-retained phase store
    /// (WebSocket clients see it via the monitor route's watch subscription).
    /// For `SavePending`/`StatsSkipped`, records the generation on the pending
    /// save so its completion/discard can clear that exact transition later.
    fn emit(&mut self, status: RecordingStatus) {
        let generation = self.recording_state.set(status);
        if matches!(status, RecordingStatus::SavePending | RecordingStatus::StatsSkipped)
            && let Some(pending) = self.pending.as_mut()
        {
            pending.phase_generation = Some(generation);
        }
    }

    /// Update the ROM/template language attached to future clip metadata. Used
    /// when monitor language auto-correction detects the other ROM language.
    pub fn set_rom_language(&mut self, rom_language: String) {
        if self.rom_language != rom_language {
            tracing::info!(from = %self.rom_language, to = %rom_language, "recording ROM language changed");
        }
        self.rom_language = rom_language;
    }

    /// Schedule the replay-buffer save for a finished run, ending report tracking.
    /// `stats` names the clip (stats-screen match, or report-screen when skipped).
    /// Any earlier pending save is flushed first so it isn't dropped.
    fn schedule_save(&mut self, now: Instant, clip_start: Instant, stats: Option<LevelMatch>) -> bool {
        self.flush_pending(now);
        let status = self.status.unwrap_or(RunStatus::Complete);
        if status.is_failed() && !self.options.save_failed_runs {
            tracing::info!("failed run reached an ending screen but failed-run saving is disabled");
            self.status = None;
            self.report = None;
            // The run is over and nothing will be saved, so drop the phase back to
            // idle ("waiting") and surface the outcome as a one-off notification
            // rather than a lingering "failed run not saved" phase.
            self.recording_state.clear();
            let _ =
                self.event_tx.send(MonitorEvent::FailedRunNotSaved { reason: FailedRunNotSavedReason::SavingDisabled });
            return false;
        }

        let save_delay = self.options.save_delay();
        let save_id = self.next_save_id;
        self.next_save_id = self.next_save_id.saturating_add(1).max(1);
        let pending = PendingSave {
            save_id,
            fire_at: now + save_delay,
            clip_start,
            finish_at: now,
            status,
            completed_at: SystemTime::now(),
            rom_language: self.rom_language.clone(),
            stats,
            time_vote: FieldVote::default(),
            target_vote: FieldVote::default(),
            best_vote: FieldVote::default(),
            stats_vote_closed: false,
            notified: false,
            phase_generation: None,
        };
        self.pending = Some(pending);
        self.sync_pending_notification(now, true);
        self.status = None;
        self.report = None;
        tracing::info!(?save_delay, "recording save scheduled");
        true
    }

    /// Reconcile the pending-save notification with the run's current savability:
    /// show or refresh it while savable (`time_changed` forces a refresh), and
    /// dismiss it once it isn't so the sticky toast can't outlive a discarded run.
    fn sync_pending_notification(&mut self, now: Instant, time_changed: bool) {
        let Some(pending) = self.pending.as_ref() else {
            return;
        };
        if pending_is_savable(&self.options, pending) {
            if !pending.notified || time_changed {
                let event = save_pending_event(pending, &self.options, now);
                let _ = self.event_tx.send(MonitorEvent::RecordingSavePending(event));
                self.pending.as_mut().unwrap().notified = true;
            }
        } else if pending.notified {
            let _ = self.event_tx.send(MonitorEvent::RecordingSaveDiscarded { save_id: pending.save_id });
            self.pending.as_mut().unwrap().notified = false;
        }
    }

    /// Build a save+trim job for the pending clip, if any, anchored to `now` as
    /// the save moment (the saved file ends at ~now, so the run is its final
    /// `elapsed` seconds). A no-op when nothing is pending.
    fn take_pending_job(&mut self, now: Instant) -> Option<SaveAndTrimJob> {
        let pending = self.pending.take()?;

        // Enforce the minimum failed-run length against the canonical time now
        // settled on `pending.stats`, so a first-frame misread can't rescue a
        // too-short run or discard a long enough one. Measured from run finish.
        if !pending_is_savable(&self.options, &pending) {
            tracing::info!(
                failed_run_length_secs =
                    failed_run_length_secs(pending.finish_at, pending.clip_start, pending.stats.as_ref()),
                minimum_failed_run_length_secs = self.options.minimum_failed_run_length_secs(),
                "failed run reached an ending screen but was shorter than the configured minimum"
            );
            // This can fire on the save timer long after the run ended, by which
            // point a new run may already be recording. Surface the outcome as a
            // notification, and clear only this save's own phase transition -- not
            // the current value, which the new run may already own.
            if let Some(generation) = pending.phase_generation {
                self.recording_state.clear_if_generation(generation);
            }
            // Guarantees the sticky "saving" toast is cleared even if this save
            // was never reconciled to unsavable earlier (normally already done).
            if pending.notified {
                let _ = self.event_tx.send(MonitorEvent::RecordingSaveDiscarded { save_id: pending.save_id });
            }
            let _ = self.event_tx.send(MonitorEvent::FailedRunNotSaved { reason: FailedRunNotSavedReason::TooShort });
            return None;
        }

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
            phase_generation: pending.phase_generation,
        })
    }

    /// Save and trim the pending clip asynchronously, if any.
    fn flush_pending(&mut self, now: Instant) {
        if let Some(job) = self.take_pending_job(now) {
            spawn_save_and_trim(job);
        }
    }

    /// When the in-flight save is due to fire, or `None` when nothing is pending.
    /// The monitor loop waits on this so the save fires on time even if captured
    /// frames stop arriving (e.g. a paused source).
    pub fn pending_fire_at(&self) -> Option<Instant> {
        self.pending.as_ref().map(|pending| pending.fire_at)
    }

    /// Fire the scheduled save once its post-run padding window has elapsed. Safe
    /// to call on any tick (frame or idle wakeup); a no-op until then.
    pub fn poll_pending(&mut self, now: Instant) {
        if self.pending.as_ref().is_some_and(|pending| now >= pending.fire_at) {
            self.flush_pending(now);
        }
    }

    /// Fold another stats reading into the in-flight save and reconcile the pending
    /// notification: refresh it on a time change, or drop it once the refined time
    /// falls below the failed-run minimum. No-op for closed votes / non-stats saves.
    fn refine_stats_vote(&mut self, now: Instant, m: &LevelMatch) {
        let time_changed = {
            let Some(pending) = self.pending.as_mut() else {
                return;
            };
            if pending.time_vote.counts.is_empty() || pending.stats_vote_closed {
                return;
            }
            record_stats_vote(pending, m)
        };
        self.sync_pending_notification(now, time_changed);
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
            // Returning to the mission grid. Meaning depends on whether the run
            // reached its report screen. A pending save from an earlier run is
            // untouched either way -- it fires on its own timer below.
            Screen::Levels => {
                if let Some(start) = self.clip_start.take() {
                    if let Some(report) = self.report.take() {
                        // Report shown, then user pressed B to the grid, bypassing stats.
                        // Run still finished, so save on the same padding timer, named from
                        // the report. Capture `status` first: `schedule_save` clears it.
                        let status = self.status.unwrap_or(RunStatus::Complete);
                        tracing::info!("stats screen skipped (report -> level select)");
                        // A discarded failed run (saving disabled) is handled inside
                        // `schedule_save`, which clears the phase and notifies; only
                        // emit a phase here when a save was actually scheduled.
                        if self.schedule_save(now, start, Some(report)) {
                            // Backing out to the grid is the *normal* ending for a failed
                            // run, so don't flag "skipped stats". Only a completed run whose
                            // stats screen was bypassed counts as skipped.
                            self.emit(if status.is_failed() {
                                RecordingStatus::SavePending
                            } else {
                                RecordingStatus::StatsSkipped
                            });
                        }
                    } else {
                        // No report screen was seen: the run was abandoned mid-play,
                        // so there's nothing worth saving.
                        self.status = None;
                        tracing::info!("recording session abandoned (returned to level select)");
                        self.emit(RecordingStatus::Cancelled);
                    }
                }
            }
            // Failure report screens flag the active run and mark it reached its
            // report screen. Emit only on the first failure frame (the screen lingers)
            // so clients see one transition; the screen picks the status/why it ended.
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
            // The mission-complete report screen: also marks the run as reaching its
            // report screen. Emit `Complete` once -- first clean report frame, or when
            // it clears an earlier failure flag. Later lingering frames don't re-emit.
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
            // The stats screen ends the run: hand it to a pending save scheduled a
            // few seconds out (so the clip captures the overlay). Taking `clip_start`
            // ends the run; later stats frames refine the time but don't re-schedule.
            Screen::Stats => {
                if let Some(start) = self.clip_start.take() {
                    tracing::info!("stats detected");
                    if self.schedule_save(now, start, Some(m.clone())) {
                        // Seed the vote with this first reading; later stats frames
                        // refine `stats` toward the most-seen time.
                        if let Some(pending) = self.pending.as_mut() {
                            record_stats_vote(pending, m);
                        }
                        self.emit(RecordingStatus::SavePending);
                    }
                    // A discarded failed run (saving disabled) is handled inside
                    // `schedule_save`, which clears the phase and notifies.
                } else {
                    // Still on the stats screen with the save in flight: keep voting
                    // the whole window so a multi-frame first misread is outvoted by
                    // the stable reading, updating the notification when it changes.
                    self.refine_stats_vote(now, m);
                }
            }
            _ => {}
        }

        // Leaving the stats screen locks the vote: any later run's stats screen
        // within the padding window must not fold into this save.
        if m.screen != Screen::Stats
            && let Some(pending) = self.pending.as_mut()
        {
            pending.stats_vote_closed = true;
        }

        // Fire the scheduled save once its post-run padding window elapses,
        // regardless of the current screen, so a pending save completes even after
        // the user backs out or starts another run.
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
    /// See [`PendingSave::phase_generation`].
    phase_generation: Option<u64>,
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
    let output_directory = replay_buffer_output_directory();
    // Hold the serialize lock across the request+wait so no second plugin save
    // races this one for OBS's identity-less saved event; released before the
    // trim, which is slow and safe to run concurrently on its own file.
    let resolved = {
        let _serialize = REPLAY_SAVE_SERIALIZE.lock().unwrap_or_else(|p| p.into_inner());
        // Snapshot the replay dir before saving so we can tell which file our save
        // wrote by what newly appears -- otherwise a user manual-save in this same
        // window could have us trim (and delete) their file instead of ours.
        let before = output_directory.as_deref().map(snapshot_replay_files);
        // Register the request (and snapshot the generation to wait past) before
        // triggering the save, so we only wake on the event this save produces and
        // `on_replay_saved` can distinguish it from the user's own manual saves.
        let since = begin_replay_save_request();
        tracing::info!("saving replay buffer");
        unsafe { crate::ffi::obs_frontend_replay_buffer_save() };

        // Block on the OBS replay-saved event (no polling); it carries the path.
        let event_path = match wait_for_replay_saved(since, REPLAY_SAVE_TIMEOUT) {
            Some(path) => path,
            None => {
                tracing::error!("replay buffer save did not complete in time");
                return;
            }
        };

        match (output_directory.as_deref(), before) {
            (Some(dir), Some(before)) => {
                let new_files = new_replay_files(dir, &before, &event_path);
                resolve_saved_replay(event_path, new_files)
            }
            // No known output directory to diff against: trust OBS's reported path.
            _ => ResolvedReplay { path: event_path, safe_to_delete: true },
        }
    };

    let ResolvedReplay { path, safe_to_delete } = resolved;
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
            if safe_to_delete {
                remove_replay_file_after_trim(&path, &saved.path);
            } else {
                tracing::warn!(
                    path = %path,
                    "keeping replay source: another replay save (e.g. the user's own) landed while this \
                     one was in flight, so the file that is ours can't be told apart"
                );
            }
            // Ignore send errors: with no WebSocket clients there are no
            // subscribers, but the save still succeeded.
            let _ = job.event_tx.send(MonitorEvent::RecordingSaved(saved));
            // Clear only this save's own phase transition, not the current value,
            // which a quick-restarted run may legitimately share for its own save.
            if let Some(generation) = job.phase_generation {
                job.recording_state.clear_if_generation(generation);
            }
        }
        Err(err) => tracing::error!("failed to trim replay clip: {err:#}"),
    }
}

/// The replay file a completed save should trim, and whether removing it
/// afterwards is safe.
struct ResolvedReplay {
    path: String,
    safe_to_delete: bool,
}

/// All regular files currently in `dir`, used as a before/after baseline to spot
/// the file a save wrote. Any read error yields an empty set (nothing looks new).
fn snapshot_replay_files(dir: &Path) -> HashSet<PathBuf> {
    let mut files = HashSet::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_file()) {
                files.insert(entry.path());
            }
        }
    }
    files
}

/// Files that appeared in `dir` since `before`, restricted to the saved file's
/// extension so unrelated churn (a concurrent trim output, say) is ignored.
fn new_replay_files(dir: &Path, before: &HashSet<PathBuf>, event_path: &str) -> Vec<PathBuf> {
    let extension = Path::new(event_path).extension().map(ToOwned::to_owned);
    snapshot_replay_files(dir)
        .into_iter()
        .filter(|path| !before.contains(path))
        .filter(|path| extension.is_none() || path.extension() == extension.as_deref())
        .collect()
}

/// Pick the file to trim from the saved event and the files that appeared during
/// the save. Exactly one new file is unambiguously ours (trust it, delete after);
/// zero or many means a concurrent save, so use OBS's path but never delete.
fn resolve_saved_replay(event_path: String, new_files: Vec<PathBuf>) -> ResolvedReplay {
    if let [only] = new_files.as_slice() {
        return ResolvedReplay { path: only.to_string_lossy().into_owned(), safe_to_delete: true };
    }
    ResolvedReplay { path: event_path, safe_to_delete: false }
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

fn remove_replay_file_after_trim(replay_path: &str, saved_path: &str) {
    let replay = Path::new(replay_path);
    let saved = Path::new(saved_path);
    if replay == saved {
        tracing::warn!(path = %replay.display(), "not deleting replay buffer file because it is also the saved clip");
        return;
    }

    match fs::remove_file(replay) {
        Ok(()) => tracing::info!(path = %replay.display(), "deleted replay buffer source file after trimming"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(path = %replay.display(), "replay buffer source file was already gone after trimming");
        }
        Err(err) => tracing::warn!(path = %replay.display(), "failed to delete replay buffer source file: {err}"),
    }
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
#[path = "recording_test.rs"]
mod recording_test;
