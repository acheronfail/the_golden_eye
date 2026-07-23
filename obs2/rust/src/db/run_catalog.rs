use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[cfg(test)]
use super::clips::ClipValidation;
use super::{clips, meta};
use crate::models::clip_metadata::{ClipMetadata, RunStatus};
use crate::youtube::{UploadHistoryEntry, YoutubeMetadata};

const DB_FILE_NAME: &str = "runs.sqlite";

#[derive(Debug, Clone)]
pub struct RunCatalogRoot {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunRetentionState {
    Pending,
    Kept,
    Expired,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunSort {
    #[default]
    Newest,
    Oldest,
    Fastest,
    Slowest,
}

impl RunRetentionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Kept => "kept",
            Self::Expired => "expired",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "pending" => Self::Pending,
            "expired" => Self::Expired,
            _ => Self::Kept,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndexedRunClip {
    pub run_id: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified: Option<SystemTime>,
    pub duration_secs: Option<f64>,
    pub metadata: ClipMetadata,
    pub retention_state: RunRetentionState,
    pub retention_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RunRecord {
    pub run_id: String,
    pub metadata: ClipMetadata,
    pub retention_state: RunRetentionState,
    pub retention_reason: Option<String>,
    pub clip: Option<IndexedRunClip>,
}

#[derive(Debug, Clone)]
pub struct RunCatalogSave {
    pub path: PathBuf,
    pub duration_secs: Option<f64>,
    pub metadata: ClipMetadata,
}

pub struct RunCatalog {
    conn: Mutex<Connection>,
    needs_seed: bool,
}

impl RunCatalog {
    pub fn path_for_settings(settings_path: &Path) -> PathBuf {
        settings_path.parent().unwrap_or_else(|| Path::new(".")).join(DB_FILE_NAME)
    }

    pub fn exists_for_settings(settings_path: &Path) -> bool {
        Self::path_for_settings(settings_path).exists()
    }

    pub fn open_for_settings(settings_path: &Path) -> anyhow::Result<Self> {
        Self::open(Self::path_for_settings(settings_path))
    }

    pub fn open(db_path: PathBuf) -> anyhow::Result<Self> {
        let existed = db_path.exists();
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating run catalog directory {}", parent.display()))?;
        }
        let conn = Connection::open(&db_path).with_context(|| format!("opening run catalog {}", db_path.display()))?;
        let reset = initialise_schema(&conn)?;
        Ok(Self { conn: Mutex::new(conn), needs_seed: !existed || reset })
    }

    pub fn needs_seed(&self) -> bool {
        self.needs_seed
    }

    #[cfg(test)]
    pub fn list(&self, roots: &[RunCatalogRoot]) -> anyhow::Result<Vec<IndexedRunClip>> {
        let runs = self.list_runs()?;
        let mut valid = Vec::new();
        for run in runs {
            let Some(clip) = run.clip else { continue };
            if !clips::is_under_roots(&clip.path, roots) {
                continue;
            }
            match clips::validate_clip(&clip) {
                ClipValidation::Unchanged => valid.push(clip),
                ClipValidation::Missing => self.detach_clip(&run.run_id, "missing")?,
                ClipValidation::Changed => match clips::read_from_disk(&clip.path) {
                    Ok(Some(updated)) => {
                        self.upsert_imported_clip(updated.clone())?;
                        valid.push(updated);
                    }
                    _ => self.detach_clip(&run.run_id, "missing")?,
                },
            }
        }
        valid.sort_by(|a, b| b.metadata.timestamp.cmp(&a.metadata.timestamp).then_with(|| b.run_id.cmp(&a.run_id)));
        Ok(valid)
    }

    pub fn list_runs(&self) -> anyhow::Result<Vec<RunRecord>> {
        clips::list_runs(&self.lock())
    }

    pub fn list_runs_sorted(&self, sort: RunSort) -> anyhow::Result<Vec<RunRecord>> {
        clips::list_runs_sorted(&self.lock(), sort)
    }

    pub fn recent_runs(&self, limit: usize) -> anyhow::Result<Vec<RunRecord>> {
        clips::recent_runs(&self.lock(), limit.clamp(1, 20))
    }

    pub fn get_run(&self, run_id: &str) -> anyhow::Result<Option<RunRecord>> {
        clips::get_run(&self.lock(), run_id)
    }

    pub fn create_finalized_run(
        &self,
        completed_at: SystemTime,
        mut metadata: ClipMetadata,
    ) -> anyhow::Result<RunRecord> {
        let completed_unix_micros = unix_micros(completed_at);
        let mut conn = self.lock();
        let is_pb = metadata.status == RunStatus::Complete
            && metadata.time_seconds.is_some()
            && metadata.level_number.is_some()
            && metadata.difficulty.as_deref().is_some_and(|value| !value.is_empty())
            && {
                let best =
                    clips::best_time(&conn, metadata.level_number.unwrap(), metadata.difficulty.as_deref().unwrap())?;
                best.is_none_or(|best| metadata.time_seconds.unwrap() < best)
            };
        metadata.retention_state = if is_pb { "kept" } else { "pending" }.to_owned();
        metadata.retention_reason = is_pb.then(|| "personalBest".to_owned());

        let base = run_id_base(completed_unix_micros, metadata.level_number, metadata.difficulty.as_deref());
        let mut run_id = base.clone();
        let mut suffix = 2;
        while clips::get_run(&conn, &run_id)?.is_some() {
            run_id = format!("{base}-{suffix}");
            suffix += 1;
        }
        metadata.run_id = run_id.clone();
        let tx = conn.transaction()?;
        clips::insert_finalized(&tx, &run_id, completed_unix_micros, &metadata)?;
        tx.commit()?;
        Ok(RunRecord {
            run_id,
            retention_state: RunRetentionState::parse(&metadata.retention_state),
            retention_reason: metadata.retention_reason.clone(),
            metadata,
            clip: None,
        })
    }

    pub fn record_saved_clip(&self, save: RunCatalogSave) -> anyhow::Result<IndexedRunClip> {
        let mut save = save;
        if save.metadata.run_id.is_empty() {
            let imported = self.prepare_imported_metadata(&save.path, save.metadata)?;
            save.metadata = imported;
        }
        if clips::get_run(&self.lock(), &save.metadata.run_id)?.is_none() {
            let completed =
                parse_timestamp_micros(&save.metadata.timestamp).unwrap_or_else(|| unix_micros(SystemTime::now()));
            clips::insert_finalized(&self.lock(), &save.metadata.run_id, completed, &save.metadata)?;
        }
        clips::attach_saved_clip(&self.lock(), &save)
    }

    pub fn keep(&self, run_id: &str) -> anyhow::Result<RunRecord> {
        let conn = self.lock();
        let mut run = clips::get_run(&conn, run_id)?.context("run not found")?;
        if run.retention_state == RunRetentionState::Kept {
            return Ok(run);
        }
        let clip = run.clip.as_ref().context("run has no video to keep")?;
        run.metadata.retention_state = "kept".to_owned();
        run.metadata.retention_reason = Some("manual".to_owned());
        crate::ffmpeg::rewrite_metadata_in_place(&clip.path, &run.metadata)?;
        let refreshed = clips::attach_saved_clip(
            &conn,
            &RunCatalogSave {
                path: clip.path.clone(),
                duration_secs: clip.duration_secs,
                metadata: run.metadata.clone(),
            },
        )?;
        run.retention_state = RunRetentionState::Kept;
        run.retention_reason = Some("manual".to_owned());
        run.clip = Some(refreshed);
        Ok(run)
    }

    pub fn update_metadata(&self, run_id: &str, mut metadata: ClipMetadata) -> anyhow::Result<RunRecord> {
        let conn = self.lock();
        let mut run = clips::get_run(&conn, run_id)?.context("run not found")?;
        metadata.run_id = run.run_id.clone();
        metadata.retention_state = run.retention_state.as_str().to_owned();
        metadata.retention_reason = run.retention_reason.clone();
        if let Some(clip) = &run.clip {
            let path = clip.path.clone();
            let duration_secs = clip.duration_secs;
            crate::ffmpeg::rewrite_metadata_in_place(&path, &metadata)?;
            clips::update_metadata(&conn, run_id, &metadata)?;
            run.clip = Some(clips::attach_saved_clip(
                &conn,
                &RunCatalogSave { path, duration_secs, metadata: metadata.clone() },
            )?);
        } else {
            clips::update_metadata(&conn, run_id, &metadata)?;
        }
        run.metadata = metadata;
        Ok(run)
    }

    pub fn delete_video_keep_history(&self, run_id: &str) -> anyhow::Result<RunRecord> {
        let conn = self.lock();
        let mut run = clips::get_run(&conn, run_id)?.context("run not found")?;
        if let Some(clip) = &run.clip {
            match fs::remove_file(&clip.path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err).with_context(|| format!("deleting {}", clip.path.display())),
            }
        }
        run.metadata.retention_state = RunRetentionState::Expired.as_str().to_owned();
        run.metadata.retention_reason = Some("deleted".to_owned());
        clips::detach_clip(&conn, run_id, RunRetentionState::Expired, "deleted", &run.metadata)?;
        clips::get_run(&conn, run_id)?.context("run disappeared after deleting video")
    }

    pub fn delete_run_and_video(&self, run_id: &str) -> anyhow::Result<()> {
        let conn = self.lock();
        let run = clips::get_run(&conn, run_id)?.context("run not found")?;
        if let Some(clip) = &run.clip {
            match fs::remove_file(&clip.path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err).with_context(|| format!("deleting {}", clip.path.display())),
            }
        }
        clips::delete_run(&conn, run_id)
    }

    pub fn cleanup_recent(&self, keep_recent: usize) -> anyhow::Result<Vec<String>> {
        let conn = self.lock();
        let runs = clips::list_runs(&conn)?;
        let mut expired = Vec::new();
        for mut run in runs.into_iter().skip(keep_recent.clamp(1, 20)) {
            if run.retention_state != RunRetentionState::Pending || run.clip.is_none() {
                continue;
            }
            let clip = run.clip.as_ref().expect("checked above");
            match fs::remove_file(&clip.path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err).with_context(|| format!("deleting {}", clip.path.display())),
            }
            run.metadata.retention_state = RunRetentionState::Expired.as_str().to_owned();
            run.metadata.retention_reason = Some("historyLimit".to_owned());
            clips::detach_clip(&conn, &run.run_id, RunRetentionState::Expired, "historyLimit", &run.metadata)?;
            expired.push(run.run_id);
        }
        Ok(expired)
    }

    pub fn resync(&self, roots: &[RunCatalogRoot]) -> anyhow::Result<()> {
        for root in roots {
            clips::ensure_directory(&root.path)?;
            for path in clips::video_files_in_directory_recursive(&root.path)? {
                let mut clip = match clips::read_from_disk(&path) {
                    Ok(Some(clip)) => clip,
                    Ok(None) => continue,
                    Err(err) => {
                        tracing::warn!(path = %path.display(), "could not read catalog clip during resync: {err:#}");
                        if let Some(existing) = clips::get_run_by_path(&self.lock(), &path)? {
                            self.detach_clip(&existing.run_id, "unreadable")?;
                        }
                        continue;
                    }
                };
                if clip.run_id.is_empty() {
                    if let Some(existing) = clips::get_run_by_path(&self.lock(), &clip.path)? {
                        clip.metadata.run_id = existing.run_id;
                        clip.metadata.retention_state = existing.retention_state.as_str().to_owned();
                        clip.metadata.retention_reason = existing.retention_reason;
                    } else {
                        clip.metadata = self.prepare_imported_metadata(&clip.path, clip.metadata)?;
                    }
                    clip.run_id = clip.metadata.run_id.clone();
                    clip.retention_state = RunRetentionState::parse(&clip.metadata.retention_state);
                    clip.retention_reason = clip.metadata.retention_reason.clone();
                    crate::ffmpeg::rewrite_metadata_in_place(&clip.path, &clip.metadata)?;
                    clip = clips::read_from_disk(&clip.path)?.context("rewritten clip metadata disappeared")?;
                }
                self.upsert_imported_clip(clip)?;
            }
        }
        for run in self.list_runs()? {
            if let Some(clip) = run.clip
                && clips::is_under_roots(&clip.path, roots)
                && !clip.path.exists()
            {
                self.detach_clip(&run.run_id, "missing")?;
            }
        }
        Ok(())
    }

    pub fn resync_and_prune(&self, roots: &[RunCatalogRoot], keep_recent: usize) -> anyhow::Result<()> {
        self.resync(roots)?;
        self.cleanup_recent(keep_recent)?;
        Ok(())
    }

    pub fn rename_path(&self, from: &Path, to: &Path) -> anyhow::Result<()> {
        clips::rename_path(&self.lock(), &clips::catalog_path(from), &clips::catalog_path(to))
    }

    pub fn remove_path(&self, path: &Path) -> anyhow::Result<()> {
        let path = clips::catalog_path(path);
        if let Some(run) =
            self.list_runs()?.into_iter().find(|run| run.clip.as_ref().is_some_and(|clip| clip.path == path))
        {
            self.detach_clip(&run.run_id, "deleted")?;
        }
        Ok(())
    }

    pub fn refresh_clip(&self, path: &Path) -> anyhow::Result<Option<IndexedRunClip>> {
        let Some(clip) = clips::read_from_disk(path)? else {
            self.remove_path(path)?;
            return Ok(None);
        };
        self.upsert_imported_clip(clip.clone())?;
        Ok(Some(clip))
    }

    pub fn youtube_history(&self) -> anyhow::Result<Vec<UploadHistoryEntry>> {
        clips::youtube_history(&self.lock())
    }

    pub fn set_youtube_history(&self, path: &Path, youtube: &YoutubeMetadata) -> anyhow::Result<()> {
        clips::set_youtube_history(&self.lock(), path, youtube)
    }

    pub fn forget_youtube_history_for_display_path(&self, display_path: &str) -> anyhow::Result<usize> {
        clips::clear_youtube_history(&self.lock(), Path::new(display_path))
    }

    fn prepare_imported_metadata(&self, path: &Path, mut metadata: ClipMetadata) -> anyhow::Result<ClipMetadata> {
        metadata.retention_state = "kept".to_owned();
        metadata.retention_reason = Some("imported".to_owned());
        let completed = parse_timestamp_micros(&metadata.timestamp).unwrap_or_else(|| {
            fs::metadata(path).and_then(|value| value.modified()).map(unix_micros).unwrap_or_default()
        });
        let base = run_id_base(completed, metadata.level_number, metadata.difficulty.as_deref());
        let mut id = base.clone();
        let mut suffix = 2;
        while self.get_run(&id)?.is_some() {
            id = format!("{base}-{suffix}");
            suffix += 1;
        }
        metadata.run_id = id;
        Ok(metadata)
    }

    fn upsert_imported_clip(&self, clip: IndexedRunClip) -> anyhow::Result<()> {
        let completed = parse_timestamp_micros(&clip.metadata.timestamp)
            .unwrap_or_else(|| clip.modified.map(unix_micros).unwrap_or_default());
        clips::upsert_imported(&self.lock(), &clip, completed)
    }

    fn detach_clip(&self, run_id: &str, reason: &str) -> anyhow::Result<()> {
        let mut run = self.get_run(run_id)?.context("run not found")?;
        run.metadata.retention_state = RunRetentionState::Expired.as_str().to_owned();
        run.metadata.retention_reason = Some(reason.to_owned());
        clips::detach_clip(&self.lock(), run_id, RunRetentionState::Expired, reason, &run.metadata)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn initialise_schema(conn: &Connection) -> anyhow::Result<bool> {
    let reset = meta::needs_reset(conn)?;
    if reset {
        clips::drop_tables(conn)?;
        meta::drop_tables(conn)?;
    }
    meta::initialise(conn)?;
    clips::initialise(conn)?;
    Ok(reset)
}

fn unix_micros(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_micros().min(i64::MAX as u128) as i64,
        Err(err) => -(err.duration().as_micros().min(i64::MAX as u128) as i64),
    }
}

fn parse_timestamp_micros(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value).ok().map(|value| value.timestamp_micros())
}

fn run_id_base(completed_unix_micros: i64, level_number: Option<i32>, difficulty: Option<&str>) -> String {
    let difficulty = difficulty
        .unwrap_or("unknown")
        .chars()
        .filter(|value| value.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    format!("{completed_unix_micros}-l{:02}-d{difficulty}", level_number.unwrap_or_default())
}

#[cfg(test)]
#[path = "run_catalog_test.rs"]
mod run_catalog_test;
