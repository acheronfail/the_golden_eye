use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
#[cfg(not(test))]
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime};

use ge_clip::{ClipMetadata, RunStatus};
use tokio::sync::broadcast;

use super::clip_output::{
    ClipOutputPolicy,
    append_extension,
    clip_metadata,
    clip_relative_path,
    ensure_output_directory,
    output_dir,
    unique_output_path,
};
#[cfg(not(test))]
use super::replay_buffer::{REPLAY_BUFFER, ReplaySaveWait};
use super::run_detection::{PendingSave, RunDetectionPolicy};
use super::{MAX_RECENT_RUN_LIMIT, RecordingSessionContext};
use crate::app::AppEvent;
use crate::cv::LevelMatch;
use crate::db::run_catalog::{RunCatalog, RunCatalogSave};
use crate::ge;
#[cfg(not(test))]
use crate::obs::replay_buffer_output_directory;
use crate::run_monitoring::RecordingStateStore;
use crate::run_monitoring::publication::{
    RecordingSavePending,
    RecordingSaved,
    ReplaySaveStage,
    ReplaySaveStateStore,
    ReplaySaveStatus,
};

/// A replay save taking this long is unusual, but OBS can still complete it.
/// Keep ownership of the request so a late identity-less event remains attached
/// to the correct run.
#[cfg(not(test))]
const REPLAY_SAVE_SLOW_WARNING: Duration = Duration::from_secs(20);
/// Avoid blocking all later saves forever if OBS never sends a completion event.
#[cfg(not(test))]
const REPLAY_SAVE_TIMEOUT: Duration = Duration::from_secs(120);
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

/// Build the provisional run event, reading `save_in_secs` as the time remaining
/// until it fires. Re-sent when the voted time is refined.
fn save_pending_event(pending: &PendingSave, policy: RunDetectionPolicy, now: Instant) -> RecordingSavePending {
    let run_length_secs = pending.finish_at.saturating_duration_since(pending.clip_start).as_secs_f64();
    let estimated_duration_secs = run_length_secs + policy.pre_run_padding_secs + policy.post_run_padding_secs;
    recording_save_pending_event(
        pending.save_id,
        pending.fire_at.saturating_duration_since(now),
        estimated_duration_secs,
        pending.status,
        pending.stats.as_ref(),
    )
}

pub(super) struct SavePipeline {
    event_tx: broadcast::Sender<AppEvent>,
    pub(super) recording_state: RecordingStateStore,
    pub(super) replay_saves: ReplaySaveStateStore,
    output_policy: ClipOutputPolicy,
    recent_run_limit: Arc<AtomicUsize>,
    source_name: String,
    run_catalog: Arc<RunCatalog>,
    monitor_session_id: Option<String>,
}

impl SavePipeline {
    pub(super) fn new(
        event_tx: broadcast::Sender<AppEvent>,
        recording_state: RecordingStateStore,
        replay_saves: ReplaySaveStateStore,
        output_policy: ClipOutputPolicy,
        recent_run_limit: usize,
        session: RecordingSessionContext,
        run_catalog: Arc<RunCatalog>,
    ) -> Self {
        Self {
            event_tx,
            recording_state,
            replay_saves,
            output_policy,
            recent_run_limit: Arc::new(AtomicUsize::new(recent_run_limit.clamp(1, MAX_RECENT_RUN_LIMIT))),
            source_name: session.source_name,
            run_catalog,
            monitor_session_id: session.monitor_session_id,
        }
    }

    pub(super) fn set_recent_run_limit_source(&mut self, source: Arc<AtomicUsize>) {
        self.recent_run_limit = source;
    }

    pub(super) fn publish_pending(&self, pending: &PendingSave, policy: RunDetectionPolicy, now: Instant) {
        let event = save_pending_event(pending, policy, now);
        self.replay_saves.schedule(ReplaySaveStatus {
            tracking_id: pending.tracking_id,
            save_id: event.save_id,
            stage: ReplaySaveStage::Scheduled,
            level: event.level.clone(),
            difficulty: event.difficulty.clone(),
            run_status: event.status.clone(),
            estimated_duration_secs: event.estimated_duration_secs,
            error: None,
        });
        let _ = self.event_tx.send(AppEvent::RecordingSavePending(event));
    }

    pub(super) fn job(&self, pending: PendingSave, now: Instant, policy: RunDetectionPolicy) -> SaveAndTrimJob {
        self.replay_saves.transition(pending.tracking_id, ReplaySaveStage::WaitingForReplaySave);

        let metadata = clip_metadata(
            pending.status,
            pending.completed_at,
            pending.stats.as_ref(),
            &self.source_name,
            &pending.game_language,
        );
        let (finalized, tracked) = match self.run_catalog.create_finalized_run_in_session(
            pending.completed_at,
            metadata.clone(),
            self.monitor_session_id.as_deref(),
        ) {
            Ok(run) => (run, true),
            Err(err) => {
                tracing::warn!(
                    "failed to record finalized run before saving clip; continuing with tagged clip: {err:#}"
                );
                (RunCatalog::untracked_finalized_run(pending.completed_at, metadata), false)
            }
        };
        if tracked {
            let _ = self.event_tx.send(AppEvent::RunCatalogChanged {
                run_id: Some(finalized.run_id.clone()),
                save_id: Some(pending.save_id),
            });
        }

        let start_before_save_secs =
            now.saturating_duration_since(pending.clip_start).as_secs_f64() + policy.pre_run_padding_secs;
        let finish_before_save_secs = now.saturating_duration_since(pending.finish_at).as_secs_f64();
        let trim_tail_secs = (finish_before_save_secs - policy.post_run_padding_secs).max(0.0);
        SaveAndTrimJob {
            tracking_id: pending.tracking_id,
            save_id: pending.save_id,
            start_before_save_secs,
            trim_tail_secs,
            status: pending.status,
            completed_at: pending.completed_at,
            stats: pending.stats,
            metadata: finalized.metadata,
            output_policy: self.output_policy.clone(),
            recent_run_limit: self.recent_run_limit.clone(),
            event_tx: self.event_tx.clone(),
            recording_state: self.recording_state.clone(),
            replay_saves: self.replay_saves.clone(),
            run_catalog: self.run_catalog.clone(),
            phase_generation: pending.phase_generation,
        }
    }

    pub(super) fn spawn(&self, pending: PendingSave, now: Instant, policy: RunDetectionPolicy) {
        spawn_save_and_trim(self.job(pending, now, policy));
    }

    #[cfg(not(test))]
    pub(super) fn flush_on_shutdown(&self, pending: PendingSave, now: Instant, policy: RunDetectionPolicy) {
        self.flush_on_shutdown_with(pending, now, policy, std::thread::sleep, save_and_trim);
    }

    pub(super) fn flush_on_shutdown_with(
        &self,
        pending: PendingSave,
        now: Instant,
        policy: RunDetectionPolicy,
        sleep: impl FnOnce(Duration),
        save: impl FnOnce(SaveAndTrimJob),
    ) {
        let save_at = if pending.fire_at > now {
            sleep(pending.fire_at.duration_since(now));
            pending.fire_at
        } else {
            now
        };
        save(self.job(pending, save_at, policy));
    }
}

/// Inputs for saving the replay buffer and trimming it to the run window on a
/// dedicated thread.
pub(super) struct SaveAndTrimJob {
    pub(super) tracking_id: u64,
    pub(super) save_id: u64,
    pub(super) start_before_save_secs: f64,
    pub(super) trim_tail_secs: f64,
    pub(super) status: RunStatus,
    pub(super) completed_at: SystemTime,
    pub(super) stats: Option<LevelMatch>,
    pub(super) metadata: ClipMetadata,
    pub(super) output_policy: ClipOutputPolicy,
    #[cfg_attr(test, allow(dead_code))]
    pub(super) recent_run_limit: Arc<AtomicUsize>,
    pub(super) event_tx: broadcast::Sender<AppEvent>,
    pub(super) recording_state: RecordingStateStore,
    pub(super) replay_saves: ReplaySaveStateStore,
    #[cfg_attr(test, allow(dead_code))]
    pub(super) run_catalog: Arc<RunCatalog>,
    /// See [`PendingSave::phase_generation`].
    pub(super) phase_generation: Option<u64>,
}

#[cfg_attr(test, allow(dead_code))]
struct TrimClipRequest<'a> {
    save_id: u64,
    replay_path: &'a str,
    start_before_save_secs: f64,
    trim_tail_secs: f64,
    status: RunStatus,
    completed_at: SystemTime,
    stats: Option<LevelMatch>,
    metadata: ClipMetadata,
    output_policy: &'a ClipOutputPolicy,
    recent_run_limit: usize,
    run_catalog: &'a RunCatalog,
}

#[cfg(not(test))]
fn save_and_trim(job: SaveAndTrimJob) {
    let output_directory = replay_buffer_output_directory();
    // Hold the serialize lock across the request+wait so no second plugin save
    // races this one for OBS's identity-less saved event; released before the
    // trim, which is slow and safe to run concurrently on its own file.
    let resolved = {
        let mut save = REPLAY_BUFFER.acquire_save();
        // Snapshot the replay dir before saving so we can tell which file our save
        // wrote by what newly appears -- otherwise a user manual-save in this same
        // window could have us trim (and delete) their file instead of ours.
        let before = output_directory.as_deref().map(snapshot_replay_files);
        // Registration precedes the OBS call, including when it completes synchronously.
        let event_path = match save.save_and_wait(
            || {
                job.replay_saves.transition(job.tracking_id, ReplaySaveStage::SavingReplay);
                tracing::info!("saving replay buffer");
                crate::obs::save_replay_buffer();
            },
            REPLAY_SAVE_SLOW_WARNING,
            REPLAY_SAVE_TIMEOUT,
        ) {
            ReplaySaveWait::Saved(path) => path,
            ReplaySaveWait::TimedOut => {
                tracing::error!(?REPLAY_SAVE_TIMEOUT, "replay buffer save did not complete in time");
                job.replay_saves.fail(job.tracking_id, "OBS replay buffer save timed out".to_owned());
                return;
            }
        };

        let resolved = match (output_directory.as_deref(), before) {
            (Some(dir), Some(before)) => {
                let new_files = new_replay_files(dir, &before, event_path.as_deref());
                resolve_saved_replay(event_path, new_files)
            }
            // No known output directory to diff against: trust OBS's reported path.
            _ => event_path.map(|path| ResolvedReplay { path, safe_to_delete: true }),
        };
        match resolved {
            Some(resolved) => resolved,
            None => {
                tracing::error!(
                    "replay buffer saved, but OBS did not report its path and the file could not be identified"
                );
                job.replay_saves
                    .fail(job.tracking_id, "OBS saved the replay but its file could not be found".to_owned());
                return;
            }
        }
    };

    let ResolvedReplay { path, safe_to_delete } = resolved;
    job.replay_saves.transition(job.tracking_id, ReplaySaveStage::Trimming);
    let run_id = job.metadata.run_id.clone();
    match trim_clip(TrimClipRequest {
        save_id: job.save_id,
        replay_path: &path,
        start_before_save_secs: job.start_before_save_secs,
        trim_tail_secs: job.trim_tail_secs,
        status: job.status,
        completed_at: job.completed_at,
        stats: job.stats,
        metadata: job.metadata,
        output_policy: &job.output_policy,
        recent_run_limit: job.recent_run_limit.load(Ordering::Acquire),
        run_catalog: &job.run_catalog,
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
            let _ = job.event_tx.send(AppEvent::RecordingSaved(saved));
            let _ = job.event_tx.send(AppEvent::RunCatalogChanged { run_id: Some(run_id), save_id: Some(job.save_id) });
            job.replay_saves.complete(job.tracking_id);
            // Clear only this save's own phase transition, not the current value,
            // which a quick-restarted run may legitimately share for its own save.
            if let Some(generation) = job.phase_generation {
                job.recording_state.handle(super::RecordingStateEvent::SaveFinished(generation));
            }
        }
        Err(err) => {
            tracing::error!("failed to trim replay clip: {err:#}");
            job.replay_saves.fail(job.tracking_id, format!("{err:#}"));
        }
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

/// Files that appeared in `dir` since `before`. When OBS reports a path, restrict
/// matches to its extension so unrelated churn is ignored.
fn new_replay_files(dir: &Path, before: &HashSet<PathBuf>, event_path: Option<&str>) -> Vec<PathBuf> {
    let extension = event_path.and_then(|path| Path::new(path).extension()).map(ToOwned::to_owned);
    snapshot_replay_files(dir)
        .into_iter()
        .filter(|path| !before.contains(path))
        .filter(|path| extension.is_none() || path.extension() == extension.as_deref())
        .collect()
}

/// One new file is ours and can be deleted after trimming. Zero or many files
/// indicate a concurrent save: use OBS's path without deleting it, or fail if absent.
fn resolve_saved_replay(event_path: Option<String>, new_files: Vec<PathBuf>) -> Option<ResolvedReplay> {
    if let [only] = new_files.as_slice() {
        return Some(ResolvedReplay { path: only.to_string_lossy().into_owned(), safe_to_delete: true });
    }
    event_path.map(|path| ResolvedReplay { path, safe_to_delete: false })
}

#[cfg(test)]
fn save_and_trim(_job: SaveAndTrimJob) {
    panic!("tests must inject save handling instead of calling OBS");
}

fn spawn_save_and_trim(job: SaveAndTrimJob) {
    let tracking_id = job.tracking_id;
    let replay_saves = job.replay_saves.clone();
    let spawned = std::thread::Builder::new().name("ge-replay-save".to_owned()).spawn(move || save_and_trim(job));
    if let Err(err) = spawned {
        tracing::error!("failed to spawn replay save thread: {err}");
        replay_saves.fail(tracking_id, format!("failed to start replay save worker: {err}"));
    }
}

/// Trim the saved replay file down to the requested run window and write it
/// alongside the replay file with a descriptive name, returning the details of
/// the written clip.
#[cfg_attr(test, allow(dead_code))]
fn trim_clip(req: TrimClipRequest<'_>) -> anyhow::Result<RecordingSaved> {
    let input = Path::new(req.replay_path);
    let duration = ge_media::duration_secs(input)?;
    // The file ends at ~the save moment. `start_before_save_secs` reaches back
    // to the detected start plus pre-run padding; `trim_tail_secs` removes any
    // extra delay beyond the requested post-run padding.
    let end = (duration - req.trim_tail_secs).clamp(0.0, duration);
    let start = (duration - req.start_before_save_secs).max(0.0).min(end);

    let failed = req.status.is_failed();
    let dir = output_dir(input, req.output_policy);
    ensure_output_directory(&dir)?;
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("replay");
    let ext = input.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    let relative_path = clip_relative_path(
        stem,
        req.status,
        req.completed_at,
        req.stats.as_ref(),
        &req.output_policy.filename_template,
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
    let clip_metadata = req.metadata;
    ge_media::trim_with_metadata(input, &output, start, end, Some(&clip_metadata))?;
    tracing::info!(output = %output.display(), "saved trimmed clip");
    match req.run_catalog.record_saved_clip(RunCatalogSave {
        path: output.clone(),
        duration_secs: Some(end - start),
        metadata: clip_metadata,
    }) {
        Ok(_) => {
            if let Err(err) = req.run_catalog.cleanup_recent(req.recent_run_limit) {
                tracing::warn!("failed to clean up expired recent-run clips: {err:#}");
            }
        }
        Err(err) => {
            tracing::warn!(path = %output.display(), "failed to update run catalog after saving clip: {err:#}");
        }
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

#[cfg(test)]
#[path = "tests/clip_saving.rs"]
mod tests;
