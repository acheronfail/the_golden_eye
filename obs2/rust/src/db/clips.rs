use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use rusqlite::{Connection, OptionalExtension, params};

use super::run_catalog::{IndexedRunClip, RunCatalogRoot, RunCatalogSave, RunRecord, RunRetentionState};
use crate::ffmpeg;
use crate::models::clip_metadata::ClipMetadata;
use crate::youtube::{UploadHistoryEntry, YoutubeMetadata};

const CREATE_TABLE: &str = include_str!("sql/clips/create_table.sql");
const CREATE_STATUS_TIMESTAMP_INDEX: &str = include_str!("sql/clips/create_status_timestamp_index.sql");
const CREATE_LEVEL_DIFFICULTY_TIMESTAMP_INDEX: &str =
    include_str!("sql/clips/create_level_difficulty_timestamp_index.sql");

pub fn initialise(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(CREATE_TABLE)?;
    conn.execute_batch(CREATE_STATUS_TIMESTAMP_INDEX)?;
    conn.execute_batch(CREATE_LEVEL_DIFFICULTY_TIMESTAMP_INDEX)?;
    conn.execute_batch("CREATE INDEX IF NOT EXISTS runs_time_idx ON runs(level_number, difficulty, time_seconds)")?;
    Ok(())
}

pub fn drop_tables(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS clips; DROP TABLE IF EXISTS runs")?;
    Ok(())
}

pub fn list_runs(conn: &Connection) -> anyhow::Result<Vec<RunRecord>> {
    let mut stmt = conn.prepare(
        "SELECT run_id, completed_unix_micros, retention_state, retention_reason,
                clip_path, size_bytes, modified_unix, duration_secs, metadata_json
         FROM runs ORDER BY completed_unix_micros DESC, run_id DESC",
    )?;
    let rows = stmt.query_map([], row_to_run)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn recent_runs(conn: &Connection, limit: usize) -> anyhow::Result<Vec<RunRecord>> {
    let mut stmt = conn.prepare(
        "SELECT run_id, completed_unix_micros, retention_state, retention_reason,
                clip_path, size_bytes, modified_unix, duration_secs, metadata_json
         FROM runs ORDER BY completed_unix_micros DESC, run_id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit as i64], row_to_run)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_run(conn: &Connection, run_id: &str) -> anyhow::Result<Option<RunRecord>> {
    conn.query_row(
        "SELECT run_id, completed_unix_micros, retention_state, retention_reason,
                clip_path, size_bytes, modified_unix, duration_secs, metadata_json
         FROM runs WHERE run_id = ?1",
        [run_id],
        row_to_run,
    )
    .optional()
    .map_err(Into::into)
}

pub fn get_run_by_path(conn: &Connection, path: &Path) -> anyhow::Result<Option<RunRecord>> {
    conn.query_row(
        "SELECT run_id, completed_unix_micros, retention_state, retention_reason,
                clip_path, size_bytes, modified_unix, duration_secs, metadata_json
         FROM runs WHERE clip_path = ?1",
        [path_to_string(&catalog_path(path))],
        row_to_run,
    )
    .optional()
    .map_err(Into::into)
}

pub fn insert_finalized(
    conn: &Connection,
    run_id: &str,
    completed_unix_micros: i64,
    metadata: &ClipMetadata,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO runs (
            run_id, completed_unix_micros, level_number, difficulty, status, time_seconds,
            retention_state, retention_reason, metadata_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            run_id,
            completed_unix_micros,
            metadata.level_number,
            metadata.difficulty,
            metadata.status.as_str(),
            metadata.time_seconds,
            metadata.retention_state,
            metadata.retention_reason,
            serde_json::to_string(metadata)?,
        ],
    )?;
    Ok(())
}

pub fn best_time(conn: &Connection, level_number: i32, difficulty: &str) -> anyhow::Result<Option<i32>> {
    Ok(conn.query_row(
        "SELECT MIN(time_seconds) FROM runs
         WHERE status = 'complete' AND level_number = ?1 AND difficulty = ?2 AND time_seconds IS NOT NULL",
        params![level_number, difficulty],
        |row| row.get(0),
    )?)
}

pub fn attach_saved_clip(conn: &Connection, save: &RunCatalogSave) -> anyhow::Result<IndexedRunClip> {
    let path = catalog_path(&save.path);
    let fs_metadata = fs::metadata(&path).with_context(|| format!("reading metadata for {}", path.display()))?;
    let clip = IndexedRunClip {
        run_id: save.metadata.run_id.clone(),
        path,
        size_bytes: fs_metadata.len(),
        modified: fs_metadata.modified().ok(),
        duration_secs: save.duration_secs,
        metadata: save.metadata.clone(),
        retention_state: RunRetentionState::parse(&save.metadata.retention_state),
        retention_reason: save.metadata.retention_reason.clone(),
    };
    let changed = conn.execute(
        "UPDATE runs SET clip_path = ?1, size_bytes = ?2, modified_unix = ?3,
             duration_secs = ?4, metadata_json = ?5, retention_state = ?6, retention_reason = ?7
         WHERE run_id = ?8",
        params![
            path_to_string(&clip.path),
            clip.size_bytes as i64,
            clip.modified.and_then(system_time_to_unix).map(|v| v as i64),
            clip.duration_secs,
            serde_json::to_string(&clip.metadata)?,
            clip.retention_state.as_str(),
            clip.retention_reason,
            clip.run_id,
        ],
    )?;
    anyhow::ensure!(changed == 1, "run not found while attaching saved clip");
    Ok(clip)
}

pub fn update_metadata(conn: &Connection, run_id: &str, metadata: &ClipMetadata) -> anyhow::Result<()> {
    let changed = conn.execute(
        "UPDATE runs SET level_number = ?1, difficulty = ?2, status = ?3, time_seconds = ?4,
             metadata_json = ?5 WHERE run_id = ?6",
        params![
            metadata.level_number,
            metadata.difficulty,
            metadata.status.as_str(),
            metadata.time_seconds,
            serde_json::to_string(metadata)?,
            run_id,
        ],
    )?;
    anyhow::ensure!(changed == 1, "run not found while updating metadata");
    Ok(())
}

pub fn upsert_imported(conn: &Connection, clip: &IndexedRunClip, completed_unix_micros: i64) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO runs (
            run_id, completed_unix_micros, level_number, difficulty, status, time_seconds,
            retention_state, retention_reason, clip_path, size_bytes, modified_unix,
            duration_secs, metadata_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(run_id) DO UPDATE SET
            clip_path = excluded.clip_path, size_bytes = excluded.size_bytes,
            modified_unix = excluded.modified_unix, duration_secs = excluded.duration_secs,
            level_number = excluded.level_number, difficulty = excluded.difficulty,
            status = excluded.status, time_seconds = excluded.time_seconds,
            metadata_json = excluded.metadata_json",
        params![
            clip.run_id,
            completed_unix_micros,
            clip.metadata.level_number,
            clip.metadata.difficulty,
            clip.metadata.status.as_str(),
            clip.metadata.time_seconds,
            clip.retention_state.as_str(),
            clip.retention_reason,
            path_to_string(&clip.path),
            clip.size_bytes as i64,
            clip.modified.and_then(system_time_to_unix).map(|v| v as i64),
            clip.duration_secs,
            serde_json::to_string(&clip.metadata)?,
        ],
    )?;
    Ok(())
}

pub fn detach_clip(
    conn: &Connection,
    run_id: &str,
    state: RunRetentionState,
    reason: &str,
    metadata: &ClipMetadata,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE runs SET clip_path = NULL, size_bytes = NULL, modified_unix = NULL,
             duration_secs = NULL, retention_state = ?1, retention_reason = ?2, metadata_json = ?3
         WHERE run_id = ?4",
        params![state.as_str(), reason, serde_json::to_string(metadata)?, run_id],
    )?;
    Ok(())
}

pub fn delete_run(conn: &Connection, run_id: &str) -> anyhow::Result<()> {
    conn.execute("DELETE FROM runs WHERE run_id = ?1", [run_id])?;
    Ok(())
}

pub fn rename_path(conn: &Connection, from: &Path, to: &Path) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE runs SET clip_path = ?1 WHERE clip_path = ?2",
        params![path_to_string(to), path_to_string(from)],
    )?;
    Ok(())
}

pub fn youtube_history(conn: &Connection) -> anyhow::Result<Vec<UploadHistoryEntry>> {
    let mut stmt = conn.prepare("SELECT clip_path, youtube_json FROM runs WHERE youtube_json IS NOT NULL")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)))?;
    rows.filter_map(|row| match row {
        Ok((Some(path), json)) => Some(Ok((path, json))),
        Ok((None, _)) => None,
        Err(err) => Some(Err(err.into())),
    })
    .map(|row: anyhow::Result<(String, String)>| {
        let (path, json) = row?;
        Ok(UploadHistoryEntry { path, youtube: serde_json::from_str::<YoutubeMetadata>(&json)? })
    })
    .collect()
}

pub fn set_youtube_history(conn: &Connection, path: &Path, youtube: &YoutubeMetadata) -> anyhow::Result<()> {
    let updated = conn.execute(
        "UPDATE runs SET youtube_json = ?1 WHERE clip_path = ?2",
        params![serde_json::to_string(youtube)?, path_to_string(&catalog_path(path))],
    )?;
    anyhow::ensure!(updated == 1, "cannot attach YouTube history to unindexed clip {}", path.display());
    Ok(())
}

pub fn clear_youtube_history(conn: &Connection, path: &Path) -> anyhow::Result<usize> {
    Ok(conn.execute(
        "UPDATE runs SET youtube_json = NULL WHERE clip_path = ?1 AND youtube_json IS NOT NULL",
        [path_to_string(&catalog_path(path))],
    )?)
}

pub fn read_from_disk(path: &Path) -> anyhow::Result<Option<IndexedRunClip>> {
    if !is_video_file(path) {
        return Ok(None);
    }
    let Some(metadata) = ffmpeg::read_clip_metadata(path)? else {
        return Ok(None);
    };
    let fs_metadata = fs::metadata(path).with_context(|| format!("reading metadata for {}", path.display()))?;
    Ok(Some(IndexedRunClip {
        run_id: metadata.run_id.clone(),
        path: catalog_path(path),
        size_bytes: fs_metadata.len(),
        modified: fs_metadata.modified().ok(),
        duration_secs: ffmpeg::duration_secs(path).ok(),
        retention_state: RunRetentionState::parse(&metadata.retention_state),
        retention_reason: metadata.retention_reason.clone(),
        metadata,
    }))
}

#[cfg(test)]
pub fn validate_clip(clip: &IndexedRunClip) -> ClipValidation {
    match fs::metadata(&clip.path) {
        Ok(metadata) if metadata.len() == clip.size_bytes && metadata.modified().ok() == clip.modified => {
            ClipValidation::Unchanged
        }
        Ok(_) => ClipValidation::Changed,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => ClipValidation::Missing,
        Err(_) => ClipValidation::Changed,
    }
}

fn row_to_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
    let run_id: String = row.get(0)?;
    let retention_state = RunRetentionState::parse(&row.get::<_, String>(2)?);
    let retention_reason: Option<String> = row.get(3)?;
    let metadata_json: String = row.get(8)?;
    let metadata = serde_json::from_str::<ClipMetadata>(&metadata_json)
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(err)))?;
    let path: Option<String> = row.get(4)?;
    let size_bytes: Option<i64> = row.get(5)?;
    let modified_unix: Option<i64> = row.get(6)?;
    let duration_secs: Option<f64> = row.get(7)?;
    let clip = path.map(|path| IndexedRunClip {
        run_id: run_id.clone(),
        path: PathBuf::from(path),
        size_bytes: size_bytes.unwrap_or_default().max(0) as u64,
        modified: modified_unix
            .and_then(|seconds| UNIX_EPOCH.checked_add(std::time::Duration::from_secs(seconds.max(0) as u64))),
        duration_secs,
        metadata: metadata.clone(),
        retention_state,
        retention_reason: retention_reason.clone(),
    });
    Ok(RunRecord { run_id, retention_state, retention_reason, metadata, clip })
}

#[cfg(test)]
pub enum ClipValidation {
    Unchanged,
    Missing,
    Changed,
}

pub fn video_files_in_directory_recursive(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    collect_video_files_recursive(dir, &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn collect_video_files_recursive(dir: &Path, paths: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    let mut entries = fs::read_dir(dir)
        .with_context(|| format!("reading directory {}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_video_files_recursive(&path, paths)?;
        } else if file_type.is_file() && is_video_file(&path) {
            paths.push(path);
        }
    }
    Ok(())
}

pub fn is_under_roots(path: &Path, roots: &[RunCatalogRoot]) -> bool {
    roots.iter().any(|root| path.starts_with(catalog_path(&root.path)))
}

pub fn ensure_directory(dir: &Path) -> anyhow::Result<()> {
    match fs::metadata(dir) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => anyhow::bail!("configured path is not a directory"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(dir).with_context(|| format!("creating run directory {}", dir.display()))
        }
        Err(err) => Err(err).with_context(|| format!("reading run directory {}", dir.display())),
    }
}

pub fn catalog_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn is_video_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if matches!(ext.as_str(), "mp4" | "mov" | "m4v" | "mkv" | "webm" | "flv" | "ts" | "avi" | "mpg" | "mpeg")
    )
}

fn system_time_to_unix(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH).ok().map(|duration| duration.as_secs())
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
