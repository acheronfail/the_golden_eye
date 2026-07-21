use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use rusqlite::{Connection, params};

use super::run_catalog::{IndexedRunClip, RunCatalogRoot, RunCatalogSave};
use crate::ffmpeg;
use crate::models::clip_metadata::ClipMetadata;
use crate::youtube::{UploadHistoryEntry, YoutubeMetadata};

const CREATE_TABLE: &str = include_str!("sql/clips/create_table.sql");
const CREATE_STATUS_TIMESTAMP_INDEX: &str = include_str!("sql/clips/create_status_timestamp_index.sql");
const CREATE_LEVEL_DIFFICULTY_TIMESTAMP_INDEX: &str =
    include_str!("sql/clips/create_level_difficulty_timestamp_index.sql");
const CREATE_TIME_INDEX: &str =
    "CREATE INDEX IF NOT EXISTS clips_time_idx ON clips(json_extract(metadata_json, '$.timeSeconds'))";
const SELECT_ALL: &str = "SELECT path, size_bytes, modified_unix, duration_secs, metadata_json FROM clips";
const UPSERT_CLIP: &str = include_str!("sql/clips/upsert.sql");
const UPDATE_PATH: &str = "UPDATE clips SET path = ?1 WHERE path = ?2";
const DELETE_PATH: &str = "DELETE FROM clips WHERE path = ?1";
const SELECT_PATHS: &str = "SELECT path FROM clips";
const SELECT_FAILED_PATHS: &str = include_str!("sql/clips/select_failed_paths.sql");
const SELECT_YOUTUBE_HISTORY: &str = include_str!("sql/clips/select_youtube_history.sql");
const UPDATE_YOUTUBE_HISTORY: &str = "UPDATE clips SET youtube_json = ?1 WHERE path = ?2";
const CLEAR_YOUTUBE_HISTORY: &str = "UPDATE clips SET youtube_json = NULL WHERE path = ?1 AND youtube_json IS NOT NULL";

pub fn initialise(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(CREATE_TABLE)?;
    conn.execute_batch(CREATE_STATUS_TIMESTAMP_INDEX)?;
    conn.execute_batch(CREATE_LEVEL_DIFFICULTY_TIMESTAMP_INDEX)?;
    conn.execute_batch(CREATE_TIME_INDEX)?;
    Ok(())
}

pub fn drop_tables(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS clips")?;
    Ok(())
}

pub fn list(conn: &Connection) -> anyhow::Result<Vec<IndexedRunClip>> {
    let mut stmt = conn.prepare(SELECT_ALL)?;
    let rows = stmt.query_map([], row_to_clip)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn record_saved(save: RunCatalogSave) -> anyhow::Result<IndexedRunClip> {
    let path = catalog_path(&save.path);
    tracing::debug!(path = %path.display(), "reading saved clip filesystem metadata");
    let fs_metadata = fs::metadata(&path).with_context(|| format!("reading metadata for {}", path.display()))?;
    Ok(IndexedRunClip {
        path,
        size_bytes: fs_metadata.len(),
        modified: fs_metadata.modified().ok(),
        duration_secs: save.duration_secs,
        metadata: save.metadata,
    })
}

pub fn read_from_disk(path: &Path) -> anyhow::Result<Option<IndexedRunClip>> {
    if !is_video_file(path) {
        return Ok(None);
    }
    tracing::debug!(path = %path.display(), "reading clip metadata from disk");
    let Some(metadata) = ffmpeg::read_clip_metadata(path)? else {
        return Ok(None);
    };
    tracing::debug!(path = %path.display(), "reading clip filesystem metadata");
    let fs_metadata = fs::metadata(path).with_context(|| format!("reading metadata for {}", path.display()))?;
    let duration_secs = ffmpeg::duration_secs(path).ok();
    Ok(Some(IndexedRunClip {
        path: catalog_path(path),
        size_bytes: fs_metadata.len(),
        modified: fs_metadata.modified().ok(),
        duration_secs,
        metadata,
    }))
}

pub fn upsert(conn: &Connection, clip: &IndexedRunClip) -> anyhow::Result<()> {
    let metadata_json = serde_json::to_string(&clip.metadata)?;
    conn.execute(
        UPSERT_CLIP,
        params![
            path_to_string(&clip.path),
            clip.size_bytes as i64,
            clip.modified.and_then(system_time_to_unix).map(|v| v as i64),
            clip.duration_secs,
            metadata_json,
        ],
    )?;
    Ok(())
}

pub fn rename_path(conn: &Connection, from: &Path, to: &Path) -> anyhow::Result<()> {
    conn.execute(UPDATE_PATH, params![path_to_string(to), path_to_string(from)])?;
    Ok(())
}

pub fn remove_path(conn: &Connection, path: &Path) -> anyhow::Result<()> {
    conn.execute(DELETE_PATH, params![path_to_string(path)])?;
    Ok(())
}

pub fn indexed_paths(conn: &Connection) -> anyhow::Result<Vec<PathBuf>> {
    let mut stmt = conn.prepare(SELECT_PATHS)?;
    let rows = stmt.query_map([], |row| Ok(PathBuf::from(row.get::<_, String>(0)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn failed_clip_paths_by_timestamp(conn: &Connection) -> anyhow::Result<Vec<PathBuf>> {
    let mut stmt = conn.prepare(SELECT_FAILED_PATHS)?;
    let rows = stmt.query_map([], |row| Ok(PathBuf::from(row.get::<_, String>(0)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn youtube_history(conn: &Connection) -> anyhow::Result<Vec<UploadHistoryEntry>> {
    let mut stmt = conn.prepare(SELECT_YOUTUBE_HISTORY)?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    let entries = rows
        .map(|row| {
            let (path, json) = row?;
            Ok(UploadHistoryEntry { path, youtube: serde_json::from_str::<YoutubeMetadata>(&json)? })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(entries)
}

pub fn set_youtube_history(conn: &Connection, path: &Path, youtube: &YoutubeMetadata) -> anyhow::Result<()> {
    let path = catalog_path(path);
    let youtube_json = serde_json::to_string(youtube)?;
    let updated = conn.execute(UPDATE_YOUTUBE_HISTORY, params![youtube_json, path_to_string(&path)])?;
    anyhow::ensure!(updated == 1, "cannot attach YouTube history to unindexed clip {}", path.display());
    Ok(())
}

pub fn clear_youtube_history(conn: &Connection, path: &Path) -> anyhow::Result<usize> {
    Ok(conn.execute(CLEAR_YOUTUBE_HISTORY, params![path_to_string(&catalog_path(path))])?)
}

pub fn validate_clip(clip: &IndexedRunClip) -> ClipValidation {
    match fs::metadata(&clip.path) {
        Ok(metadata) => {
            let modified = metadata.modified().ok();
            if metadata.len() == clip.size_bytes && modified == clip.modified {
                ClipValidation::Unchanged
            } else {
                ClipValidation::Changed
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => ClipValidation::Missing,
        Err(_) => ClipValidation::Changed,
    }
}

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
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("reading directory {}", dir.display()))? {
        entries.push(entry?);
    }
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
            tracing::info!(path = %dir.display(), "creating configured run directory");
            fs::create_dir_all(dir).with_context(|| format!("creating run directory {}", dir.display()))?;
            Ok(())
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
        Some(ext)
            if matches!(
                ext.as_str(),
                "mp4" | "mov" | "m4v" | "mkv" | "webm" | "flv" | "ts" | "avi" | "mpg" | "mpeg"
            )
    )
}

fn row_to_clip(row: &rusqlite::Row<'_>) -> rusqlite::Result<IndexedRunClip> {
    let path = PathBuf::from(row.get::<_, String>(0)?);
    let size_bytes: i64 = row.get(1)?;
    let modified_unix: Option<i64> = row.get(2)?;
    let duration_secs: Option<f64> = row.get(3)?;
    let metadata_json: String = row.get(4)?;
    let metadata = serde_json::from_str::<ClipMetadata>(&metadata_json)
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(err)))?;
    Ok(IndexedRunClip {
        path,
        size_bytes: size_bytes.max(0) as u64,
        modified: modified_unix
            .and_then(|seconds| UNIX_EPOCH.checked_add(std::time::Duration::from_secs(seconds.max(0) as u64))),
        duration_secs,
        metadata,
    })
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn system_time_to_unix(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH).ok().map(|duration| duration.as_secs())
}
