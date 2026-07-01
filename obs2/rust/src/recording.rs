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

use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use anyhow::Context;
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::cv::{LevelMatch, Screen};
use crate::ffmpeg;
use crate::http::{MonitorEvent, RecordingSaved, RecordingStatus};

/// Default filename template for trimmed clips. Mirrors the frontend default and
/// preserves the original naming scheme unless the user overrides it.
const DEFAULT_CLIP_FILENAME_TEMPLATE: &str = "{replay} - clip - {level}{time_suffix}{failed_suffix}";
const DEFAULT_POST_RUN_PADDING_SECS: f64 = 5.0;

/// How long to wait for OBS to finish writing the saved replay file before
/// giving up. The save is asynchronous; we block on the replay-saved event
/// (delivered via [`on_replay_saved`]) rather than polling.
const REPLAY_SAVE_TIMEOUT: Duration = Duration::from_secs(20);

/// Recording behaviour supplied by the frontend when a monitor session starts.
/// Empty output paths preserve the old behaviour: write the trimmed clip beside
/// OBS's replay-buffer file.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RecordingOptions {
    pub completed_output_path: String,
    pub save_failed_runs: bool,
    pub failed_output_path: String,
    /// Number of failed clips to keep in the failed output directory. 0 means
    /// unlimited.
    pub failed_run_limit: usize,
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
            clip_filename_template: DEFAULT_CLIP_FILENAME_TEMPLATE.to_owned(),
            pre_run_padding_secs: 0.0,
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
        Self::non_negative_secs(self.pre_run_padding_secs, 0.0)
    }

    fn post_run_padding_secs(&self) -> f64 {
        Self::non_negative_secs(self.post_run_padding_secs, DEFAULT_POST_RUN_PADDING_SECS)
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
        let (next, res) = REPLAY_SAVED_CV.wait_timeout(guard, timeout - elapsed).unwrap_or_else(|p| p.into_inner());
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
    /// When the post-run padding window elapses and we save the buffer.
    fire_at: Instant,
    /// When the run began -- the anchor for where the trimmed clip starts.
    clip_start: Instant,
    /// When the run ending was detected -- the anchor for post-run padding.
    finish_at: Instant,
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
    /// Broadcasts a [`MonitorEvent::RecordingSaved`] to WebSocket clients once a
    /// clip is written. Cloned into each save thread.
    event_tx: broadcast::Sender<MonitorEvent>,
    /// Recording/output options fixed for this monitor session.
    options: RecordingOptions,
}

impl RecordingState {
    pub fn new(event_tx: broadcast::Sender<MonitorEvent>, options: RecordingOptions) -> Self {
        RecordingState { clip_start: None, failed: false, report: None, pending: None, event_tx, options }
    }

    /// Broadcast a recorder state transition to connected WebSocket clients.
    /// Send errors (no subscribers) are ignored -- the state change stands
    /// regardless of whether anyone is listening.
    fn emit(&self, status: RecordingStatus) {
        let _ = self.event_tx.send(MonitorEvent::RecordingState { status });
    }

    /// Schedule the replay-buffer save for a finished run, ending the active
    /// run's report tracking. `stats` names the clip -- the stats-screen match
    /// on the normal path, or the report-screen match when the stats screen was
    /// skipped. Any earlier pending save is flushed first so it isn't dropped.
    fn schedule_save(&mut self, now: Instant, clip_start: Instant, stats: Option<LevelMatch>) -> bool {
        self.flush_pending(now);
        if self.failed && !self.options.save_failed_runs {
            tracing::info!("failed run reached an ending screen but failed-run saving is disabled");
            self.failed = false;
            self.report = None;
            return false;
        }
        let save_delay = self.options.save_delay();
        self.pending =
            Some(PendingSave { fire_at: now + save_delay, clip_start, finish_at: now, failed: self.failed, stats });
        self.failed = false;
        self.report = None;
        tracing::info!(?save_delay, "recording save scheduled");
        true
    }

    /// Save and trim the pending clip, if any, anchored to `now` as the save
    /// moment (the saved file ends at ~now, so the run is its final `elapsed`
    /// seconds). A no-op when nothing is pending.
    fn flush_pending(&mut self, now: Instant) {
        if let Some(pending) = self.pending.take() {
            let start_before_save_secs =
                now.saturating_duration_since(pending.clip_start).as_secs_f64() + self.options.pre_run_padding_secs();
            let finish_before_save_secs = now.saturating_duration_since(pending.finish_at).as_secs_f64();
            let trim_tail_secs = (finish_before_save_secs - self.options.post_run_padding_secs()).max(0.0);
            spawn_save_and_trim(
                start_before_save_secs,
                trim_tail_secs,
                pending.failed,
                pending.stats,
                self.options.clone(),
                self.event_tx.clone(),
            );
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
                    self.report = None;
                    ensure_replay_buffer_running();
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
                        // `schedule_save` clears `failed`, so capture it first.
                        let failed = self.failed;
                        tracing::info!("stats screen skipped (report -> level select)");
                        let scheduled = self.schedule_save(now, start, Some(report));
                        // Backing out to the grid is the *normal* ending for a
                        // failed run, so don't flag "skipped stats" -- just move to
                        // the saving state. Only a completed run whose stats screen
                        // was bypassed counts as skipped.
                        self.emit(if scheduled {
                            if failed { RecordingStatus::SavePending } else { RecordingStatus::StatsSkipped }
                        } else {
                            RecordingStatus::FailedDiscarded
                        });
                    } else {
                        // No report screen was seen: the run was abandoned mid-play,
                        // so there's nothing worth saving.
                        self.failed = false;
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
                    if !self.failed {
                        self.failed = true;
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
                    if first_report || self.failed {
                        self.failed = false;
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

/// Save the replay buffer and trim it to the last `elapsed` seconds, on a
/// dedicated thread so the (blocking) save-wait and remux never stall the
/// monitor's frame loop. On success, broadcasts a [`MonitorEvent::RecordingSaved`]
/// over `event_tx` describing the written clip.
fn spawn_save_and_trim(
    start_before_save_secs: f64,
    trim_tail_secs: f64,
    failed: bool,
    stats: Option<LevelMatch>,
    options: RecordingOptions,
    event_tx: broadcast::Sender<MonitorEvent>,
) {
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

        match trim_clip(&path, start_before_save_secs, trim_tail_secs, failed, stats, &options) {
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

/// Trim the saved replay file down to the requested run window and write it
/// alongside the replay file with a descriptive name, returning the details of
/// the written clip.
fn trim_clip(
    replay_path: &str,
    start_before_save_secs: f64,
    trim_tail_secs: f64,
    failed: bool,
    stats: Option<LevelMatch>,
    options: &RecordingOptions,
) -> anyhow::Result<RecordingSaved> {
    let input = Path::new(replay_path);
    let duration = ffmpeg::duration_secs(input)?;
    // The file ends at ~the save moment. `start_before_save_secs` reaches back
    // to the detected start plus pre-run padding; `trim_tail_secs` removes any
    // extra delay beyond the requested post-run padding.
    let end = (duration - trim_tail_secs).clamp(0.0, duration);
    let start = (duration - start_before_save_secs).max(0.0).min(end);

    let dir = output_dir(input, failed, options);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating output directory {}", dir.display()))?;
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("replay");
    let ext = input.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    let name = clip_name(stem, failed, stats.as_ref(), options.clip_filename_template());
    let output = unique_output_path(&dir.join(format!("{name}.{ext}")));

    tracing::info!(
        input = %input.display(),
        output = %output.display(),
        start,
        end = duration,
        trim_end = end,
        duration,
        failed,
        "trimming replay clip",
    );
    ffmpeg::trim(input, &output, start, end)?;
    tracing::info!(output = %output.display(), "saved trimmed clip");
    if failed
        && let Err(err) =
            prune_failed_clips(output.parent().unwrap_or_else(|| Path::new(".")), options.failed_run_limit, &output)
    {
        tracing::warn!("failed to prune old failed clips: {err:#}");
    }

    Ok(RecordingSaved {
        path: output.to_string_lossy().into_owned(),
        replay_path: replay_path.to_owned(),
        // The clip spans [start, end]; clamping `start` above means this is the
        // buffer length when the run outran it, otherwise the configured window.
        duration_secs: end - start,
        failed,
        stats,
    })
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

fn failed_manifest_path(dir: &Path) -> PathBuf {
    dir.join(".the-golden-eye-failed-clips.json")
}

fn read_failed_manifest(dir: &Path) -> Vec<PathBuf> {
    let path = failed_manifest_path(dir);
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    serde_json::from_slice::<Vec<String>>(&bytes).unwrap_or_default().into_iter().map(PathBuf::from).collect()
}

fn write_failed_manifest(dir: &Path, paths: &[PathBuf]) -> anyhow::Result<()> {
    let values: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let bytes = serde_json::to_vec_pretty(&values)?;
    std::fs::write(failed_manifest_path(dir), bytes)
        .with_context(|| format!("writing failed clip manifest in {}", dir.display()))
}

fn prune_failed_clips(dir: &Path, keep: usize, saved_path: &Path) -> anyhow::Result<()> {
    if keep == 0 {
        return Ok(());
    }

    let mut paths = read_failed_manifest(dir);
    paths.push(saved_path.to_path_buf());

    for entry in std::fs::read_dir(dir).with_context(|| format!("reading failed clip directory {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !is_failed_clip_path(&path) {
            continue;
        }
        paths.push(path);
    }

    paths.sort();
    paths.dedup();

    let mut clips = Vec::new();
    for path in paths {
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.is_file() {
            clips.push((metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH), path));
        }
    }
    clips.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    let kept: Vec<PathBuf> = clips.iter().take(keep).map(|(_, path)| path.clone()).collect();
    for (_, path) in clips.into_iter().skip(keep) {
        tracing::info!(path = %path.display(), "pruning old failed clip");
        std::fs::remove_file(&path).with_context(|| format!("removing old failed clip {}", path.display()))?;
    }
    write_failed_manifest(dir, &kept)?;

    Ok(())
}

fn is_failed_clip_path(path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem.contains(" - clip") && stem.ends_with(" - failed"))
}

/// Build an output file name from the configured template and matched level info.
/// The default template includes the replay stem, which already carries OBS's
/// timestamp, so clips stay distinct without adding another wall clock.
fn clip_name(stem: &str, failed: bool, stats: Option<&LevelMatch>, template: &str) -> String {
    let rendered = render_clip_template(template, stem, failed, stats);
    let sanitized = sanitize_filename(&rendered);
    if sanitized.is_empty() {
        sanitize_filename(&render_clip_template(DEFAULT_CLIP_FILENAME_TEMPLATE, stem, failed, stats))
    } else {
        sanitized
    }
}

fn render_clip_template(template: &str, stem: &str, failed: bool, stats: Option<&LevelMatch>) -> String {
    let mission =
        stats.map(|m| if m.mission >= 0 { format!("{:02}", m.mission) } else { "??".to_owned() }).unwrap_or_default();
    let part = stats.map(|m| if m.part >= 0 { m.part.to_string() } else { "?".to_owned() }).unwrap_or_default();
    let difficulty =
        stats.map(|m| if m.difficulty >= 0 { m.difficulty.to_string() } else { "?".to_owned() }).unwrap_or_default();
    let level = stats.map(|_| format!("m{mission}-{part} d{difficulty}")).unwrap_or_else(|| "unknown".to_owned());
    let time = stats.and_then(|m| m.times.map(|times| times.time.to_string())).unwrap_or_default();
    let time_suffix = if time.is_empty() { String::new() } else { format!(" - {time}s") };
    let status = if failed { "failed" } else { "complete" };
    let failed_suffix = if failed { " - failed" } else { "" };

    template
        .replace("{replay}", stem)
        .replace("{mission}", &mission)
        .replace("{part}", &part)
        .replace("{difficulty}", &difficulty)
        .replace("{level}", &level)
        .replace("{time}", &time)
        .replace("{time_suffix}", &time_suffix)
        .replace("{status}", status)
        .replace("{failed_suffix}", failed_suffix)
}

fn sanitize_filename(name: &str) -> String {
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
    use std::fs;
    use std::io;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

    fn wait_for_distinct_mtime() {
        thread::sleep(Duration::from_millis(25));
    }

    fn match_with_time() -> LevelMatch {
        LevelMatch {
            screen: Screen::Stats,
            mission: 5,
            part: 1,
            difficulty: 2,
            times: Some(Times { time: 123, target_time: Some(100), best_time: Some(130) }),
            raw_times: vec![123, 100, 130],
            runtime_ms: 0.0,
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
    fn clip_template_renders_and_sanitizes_filenames() {
        let m = match_with_time();

        let rendered = render_clip_template(
            "{mission}-{part}-{difficulty}-{level}-{time}-{status}{failed_suffix}",
            "obs replay",
            true,
            Some(&m),
        );
        assert_eq!(rendered, "05-1-2-m05-1 d2-123-failed - failed");

        let name = clip_name("OBS/Replay:01", true, Some(&m), "../{replay}/{level}:{time}?{failed_suffix}");
        for forbidden in ['/', '\\', ':', '*', '?', '"', '<', '>', '|'] {
            assert!(!name.contains(forbidden), "{name:?} still contains {forbidden:?}");
        }
        assert!(name.contains("OBS-Replay-01"));
        assert!(name.contains("m05-1 d2"));
        assert!(name.ends_with("- failed"));

        assert_eq!(clip_name("replay", false, None, "..."), "replay - clip - unknown");
    }

    #[test]
    fn read_failed_manifest_treats_missing_or_invalid_manifest_as_empty() {
        let dir = TestDir::new("manifest-empty");
        assert!(read_failed_manifest(dir.path()).is_empty());

        fs::write(failed_manifest_path(dir.path()), b"not json").unwrap();
        assert!(read_failed_manifest(dir.path()).is_empty());
    }

    #[test]
    fn prune_failed_clips_keep_zero_is_unlimited_and_deletes_nothing() {
        let dir = TestDir::new("prune-unlimited");
        let old = dir.join("obs - clip - old - failed.mp4");
        let saved = dir.join("obs - clip - saved - failed.mp4");
        write_file(&old);
        write_file(&saved);

        prune_failed_clips(dir.path(), 0, &saved).unwrap();

        assert!(old.exists());
        assert!(saved.exists());
        assert!(!failed_manifest_path(dir.path()).exists());
    }

    #[test]
    fn prune_failed_clips_keeps_newest_manifest_entries_and_leaves_untracked_files() {
        let dir = TestDir::new("prune-manifest");
        let old = dir.join("custom-old.mp4");
        let newer = dir.join("custom-newer.mp4");
        let unrelated = dir.join("family-video.mp4");
        let saved = dir.join("custom-saved.mp4");

        write_file(&old);
        wait_for_distinct_mtime();
        write_file(&newer);
        wait_for_distinct_mtime();
        write_file(&unrelated);
        write_failed_manifest(dir.path(), &[old.clone(), newer.clone()]).unwrap();
        wait_for_distinct_mtime();
        write_file(&saved);

        prune_failed_clips(dir.path(), 2, &saved).unwrap();

        assert!(!old.exists(), "old manifest-tracked clip should be pruned");
        assert!(newer.exists());
        assert!(saved.exists());
        assert!(unrelated.exists(), "untracked files must never be pruned");

        let manifest = read_failed_manifest(dir.path());
        assert_eq!(manifest.len(), 2);
        assert!(manifest.contains(&newer));
        assert!(manifest.contains(&saved));
    }

    #[test]
    fn prune_failed_clips_discovers_default_failed_names_without_manifest() {
        let dir = TestDir::new("prune-discovered");
        let old = dir.join("obs - clip - m01 - failed.mp4");
        let not_failed = dir.join("obs - clip - m02.mp4");
        let saved = dir.join("obs - clip - m03 - failed.mp4");

        write_file(&old);
        wait_for_distinct_mtime();
        write_file(&not_failed);
        wait_for_distinct_mtime();
        write_file(&saved);

        prune_failed_clips(dir.path(), 1, &saved).unwrap();

        assert!(!old.exists(), "older default failed clip should be pruned");
        assert!(saved.exists());
        assert!(not_failed.exists(), "non-failed clip should not be pruned");

        assert_eq!(read_failed_manifest(dir.path()), vec![saved]);
    }
}
