//! Replay-buffer driven recording.
//!
//! Rather than start/stop a fresh recording per run (the legacy approach, which
//! risked clipping the start while the recorder spun up), we keep OBS's replay
//! buffer running for the whole session and save a window out of it at the end.
//! [`RecordingState`] is fed every matched frame; it tracks where a run begins
//! and ends and, a few seconds after the post-mission stats screen appears,
//! saves the replay buffer and trims it (via [`crate::ffmpeg`]) down to just the
//! run.
//!
//! Timing is anchored to the moment the buffer is saved: the saved file ends at
//! ~"now", so a run that started `elapsed` seconds ago occupies the final
//! `elapsed` seconds of the file. We trim `[duration - elapsed, duration]`.

use std::path::Path;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::broadcast;

use crate::cv::{LevelMatch, Screen};
use crate::ffmpeg;
use crate::http::{MonitorEvent, RecordingSaved};

/// How long after the stats screen first appears to keep recording before
/// saving, so the clip includes a few seconds of the stats overlay. The spec
/// notes this will become configurable; for now it is fixed.
const STATS_LINGER: Duration = Duration::from_secs(5);

/// How long to wait for OBS to finish writing the saved replay file before
/// giving up. The save is asynchronous; we block on the replay-saved event
/// (delivered via [`on_replay_saved`]) rather than polling.
const REPLAY_SAVE_TIMEOUT: Duration = Duration::from_secs(20);

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

/// The current event generation. Snapshotted *before* triggering a save so the
/// subsequent wait only resolves on a new event, never one already delivered.
fn replay_saved_generation() -> u64 {
    REPLAY_SAVED.lock().unwrap_or_else(|p| p.into_inner()).generation
}

/// Block until a replay-saved event newer than `since` arrives, returning the
/// path OBS wrote, or `None` on timeout (or if the event carried no path).
fn wait_for_replay_saved(since: u64, timeout: Duration) -> Option<String> {
    let start = Instant::now();
    let mut guard = REPLAY_SAVED.lock().unwrap_or_else(|p| p.into_inner());
    while guard.generation == since {
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return None;
        }
        let (next, res) =
            REPLAY_SAVED_CV.wait_timeout(guard, timeout - elapsed).unwrap_or_else(|p| p.into_inner());
        guard = next;
        if res.timed_out() {
            return None;
        }
    }
    guard.last_path.clone()
}

/// Whether the replay buffer is enabled in the active profile (the OBS "Enable
/// Replay Buffer" checkbox). Distinct from [`replay_buffer_active`].
pub fn replay_buffer_enabled() -> bool {
    unsafe { crate::ffi::ge_obs_replay_buffer_enabled() }
}

/// Whether the replay buffer output is currently running.
pub fn replay_buffer_active() -> bool {
    unsafe { crate::ffi::obs_frontend_replay_buffer_active() }
}

/// Start the replay buffer if it is enabled and not already running.
pub fn ensure_replay_buffer_running() {
    if !replay_buffer_enabled() {
        tracing::warn!("replay buffer is not enabled in OBS; recording will not work");
        return;
    }
    if !replay_buffer_active() {
        tracing::info!("starting replay buffer");
        unsafe { crate::ffi::obs_frontend_replay_buffer_start() };
    }
}

/// A save that has been scheduled and *will* happen, captured in full the moment
/// the stats screen is seen. It is intentionally decoupled from the active-run
/// state below: once scheduled it owns everything it needs, so backing out to
/// the level grid or immediately starting another run can never drop it -- it
/// still fires on its own timer.
struct PendingSave {
    /// When the linger window elapses and we save the buffer.
    fire_at: Instant,
    /// When the run began -- the anchor for where the trimmed clip starts.
    clip_start: Instant,
    /// Whether a failure screen was seen during the run (for naming/logging).
    failed: bool,
    /// The stats-screen match, kept for naming the output clip.
    stats: Option<LevelMatch>,
}

/// Tracks one recording session as it moves through the on-screen states, and
/// drives the replay-buffer save + trim when a run finishes. Fed one matched
/// frame at a time via [`RecordingState::on_frame`].
pub struct RecordingState {
    /// When the currently-active run began, or `None` when no run is in
    /// progress. A scheduled save lives in `pending` instead, so it survives the
    /// active run ending.
    clip_start: Option<Instant>,
    /// Whether a failure screen (abort / failed / KIA) was seen during the
    /// active run. Tracked for naming/logging; the clip is saved either way.
    failed: bool,
    /// A scheduled save in flight, if any. Independent of the active run: once
    /// set it is always saved when its timer elapses, even if the user backs out
    /// or starts another run in the meantime.
    pending: Option<PendingSave>,
    /// Broadcasts a [`MonitorEvent::RecordingSaved`] to WebSocket clients once a
    /// clip is written. Cloned into each save thread.
    event_tx: broadcast::Sender<MonitorEvent>,
}

impl RecordingState {
    pub fn new(event_tx: broadcast::Sender<MonitorEvent>) -> Self {
        RecordingState { clip_start: None, failed: false, pending: None, event_tx }
    }

    /// Save and trim the pending clip, if any, anchored to `now` as the save
    /// moment (the saved file ends at ~now, so the run is its final `elapsed`
    /// seconds). A no-op when nothing is pending.
    fn flush_pending(&mut self, now: Instant) {
        if let Some(pending) = self.pending.take() {
            let elapsed = now.saturating_duration_since(pending.clip_start).as_secs_f64();
            spawn_save_and_trim(elapsed, pending.failed, pending.stats, self.event_tx.clone());
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
                    self.failed = false;
                    ensure_replay_buffer_running();
                    tracing::info!("recording session started");
                }
            }
            // Backing out to the mission grid abandons the *active* run (it never
            // reached stats, so there's nothing to save). A pending save is
            // deliberately untouched -- it still fires on its timer below.
            Screen::Levels => {
                if self.clip_start.take().is_some() {
                    self.failed = false;
                    tracing::info!("recording session abandoned (returned to level select)");
                }
            }
            // Failure screens just flag the active run; it still ends at stats.
            Screen::Failed | Screen::Abort | Screen::Kia => {
                if self.clip_start.is_some() {
                    self.failed = true;
                }
            }
            Screen::Complete => {
                if self.clip_start.is_some() {
                    self.failed = false;
                }
            }
            // The stats screen ends the run: hand the active run to a pending save
            // scheduled a few seconds out (so the clip captures the overlay).
            // Taking `clip_start` ends the active run, so later stats frames don't
            // re-schedule and a fresh run can begin right away. Any save still
            // waiting from an earlier run is flushed first so it isn't dropped.
            Screen::Stats => {
                if let Some(start) = self.clip_start.take() {
                    self.flush_pending(now);
                    tracing::info!("stats detected; saving replay buffer in {:?}", STATS_LINGER);
                    self.pending = Some(PendingSave {
                        fire_at: now + STATS_LINGER,
                        clip_start: start,
                        failed: self.failed,
                        stats: Some(m.clone()),
                    });
                    self.failed = false;
                }
            }
            _ => {}
        }

        // Fire the scheduled save once its linger window elapses. This runs every
        // frame regardless of the current screen, so a pending save completes
        // even after the user backs out or starts another run.
        if let Some(pending) = &self.pending {
            if now >= pending.fire_at {
                self.flush_pending(now);
            }
        }
    }
}

/// Save the replay buffer and trim it to the last `elapsed` seconds, on a
/// dedicated thread so the (blocking) save-wait and remux never stall the
/// monitor's frame loop. On success, broadcasts a [`MonitorEvent::RecordingSaved`]
/// over `event_tx` describing the written clip.
fn spawn_save_and_trim(elapsed: f64, failed: bool, stats: Option<LevelMatch>, event_tx: broadcast::Sender<MonitorEvent>) {
    let spawned = std::thread::Builder::new().name("ge-replay-save".to_owned()).spawn(move || {
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

        match trim_clip(&path, elapsed, failed, stats) {
            Ok(saved) => {
                // Ignore send errors: with no WebSocket clients there are no
                // subscribers, but the save still succeeded.
                let _ = event_tx.send(MonitorEvent::RecordingSaved(saved));
            }
            Err(err) => tracing::error!("failed to trim replay clip: {err:#}"),
        }
    });
    if let Err(err) = spawned {
        tracing::error!("failed to spawn replay save thread: {err}");
    }
}

/// Trim the saved replay file down to the last `elapsed` seconds (the run) and
/// write it alongside the replay file with a descriptive name, returning the
/// details of the written clip.
fn trim_clip(replay_path: &str, elapsed: f64, failed: bool, stats: Option<LevelMatch>) -> anyhow::Result<RecordingSaved> {
    let input = Path::new(replay_path);
    let duration = ffmpeg::duration_secs(input)?;
    // The file ends at ~the save moment, so the run is its final `elapsed`
    // seconds. Clamp the start in case the run outran the buffer's length.
    let start = (duration - elapsed).max(0.0);

    let dir = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("replay");
    let ext = input.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    let name = clip_name(stem, failed, stats.as_ref());
    let output = dir.join(format!("{name}.{ext}"));

    tracing::info!(
        input = %input.display(),
        output = %output.display(),
        start,
        end = duration,
        duration,
        failed,
        "trimming replay clip",
    );
    ffmpeg::trim(input, &output, start, duration)?;
    tracing::info!(output = %output.display(), "saved trimmed clip");

    Ok(RecordingSaved {
        path: output.to_string_lossy().into_owned(),
        replay_path: replay_path.to_owned(),
        // The clip spans [start, duration]; clamping `start` above means this is
        // the buffer length when the run outran it, otherwise `elapsed`.
        duration_secs: duration - start,
        failed,
        stats,
    })
}

/// Build an output file name from the replay file stem plus, when available, the
/// matched level info -- enough to tell clips apart without a wall clock (the
/// replay stem already carries OBS's timestamp).
fn clip_name(stem: &str, failed: bool, stats: Option<&LevelMatch>) -> String {
    let mut name = format!("{stem} - clip");
    if let Some(m) = stats {
        name.push_str(&format!(" - m{:02}-{} d{}", m.mission, m.part, m.difficulty));
        if let Some(time) = m.times.first() {
            name.push_str(&format!(" - {time}s"));
        }
    }
    if failed {
        name.push_str(" - failed");
    }
    name
}
