use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use rusqlite::{Connection, OptionalExtension, params};

use super::run_catalog::{IndexedRunClip, RunCatalogRoot, RunCatalogSave, RunRecord, RunRetentionState, RunSort};
use crate::ffmpeg;
use crate::models::clip_metadata::{ClipMetadata, RunStatus};
use crate::youtube::{UploadHistoryEntry, YoutubeMetadata};

const CREATE_TABLE: &str = include_str!("sql/runs/create_table.sql");
const CREATE_STATUS_TIMESTAMP_INDEX: &str = include_str!("sql/runs/create_status_timestamp_index.sql");
const CREATE_LEVEL_DIFFICULTY_TIMESTAMP_INDEX: &str =
    include_str!("sql/runs/create_level_difficulty_timestamp_index.sql");
const CREATE_TIME_INDEX: &str = include_str!("sql/runs/create_time_index.sql");
const CREATE_TIME_SORT_INDEX: &str = include_str!("sql/runs/create_time_sort_index.sql");
const DROP_TABLES: &str = include_str!("sql/runs/drop_tables.sql");
const MIGRATE_V2_ROWS: &str = include_str!("sql/runs/migrate_v2_rows.sql");
const CREATE_SESSION_TABLES: &str = include_str!("sql/runs/create_session_tables.sql");
const LIST_RUNS_NEWEST: &str = include_str!("sql/runs/list_runs_newest.sql");
const LIST_RUNS_OLDEST: &str = include_str!("sql/runs/list_runs_oldest.sql");
const LIST_RUNS_FASTEST: &str = include_str!("sql/runs/list_runs_fastest.sql");
const LIST_RUNS_SLOWEST: &str = include_str!("sql/runs/list_runs_slowest.sql");
const RECENT_RUNS: &str = include_str!("sql/runs/recent_runs.sql");
const GET_RUN: &str = include_str!("sql/runs/get_run.sql");
const GET_RUN_BY_PATH: &str = include_str!("sql/runs/get_run_by_path.sql");
const INSERT_FINALIZED: &str = include_str!("sql/runs/insert_finalized.sql");
const BEST_TIME: &str = include_str!("sql/runs/best_time.sql");
const STATISTIC_FACTS: &str = include_str!("sql/runs/statistic_facts.sql");
const TOTAL_MONITOR_SESSION_MICROS: &str = include_str!("sql/runs/total_monitor_session_micros.sql");
const COMBINED_BEST_ROWS: &str = include_str!("sql/runs/combined_best_rows.sql");
const INSERT_MONITOR_SESSION: &str = include_str!("sql/runs/insert_monitor_session.sql");
const END_MONITOR_SESSION: &str = include_str!("sql/runs/end_monitor_session.sql");
const DELETE_EMPTY_MONITOR_SESSION: &str = include_str!("sql/runs/delete_empty_monitor_session.sql");
const RECONCILE_MONITOR_SESSIONS: &str = include_str!("sql/runs/reconcile_monitor_sessions.sql");
const MONITOR_SESSIONS: &str = include_str!("sql/runs/monitor_sessions.sql");
const MONITOR_SESSION: &str = include_str!("sql/runs/monitor_session.sql");
const MONITOR_SESSION_FACTS: &str = include_str!("sql/runs/monitor_session_facts.sql");
const ATTACH_SAVED_CLIP: &str = include_str!("sql/runs/attach_saved_clip.sql");
const UPDATE_METADATA: &str = include_str!("sql/runs/update_metadata.sql");
const UPSERT_IMPORTED: &str = include_str!("sql/runs/upsert_imported.sql");
const DETACH_CLIP: &str = include_str!("sql/runs/detach_clip.sql");

#[derive(Debug, Clone)]
pub struct RunStatisticFact {
    pub run_id: String,
    pub completed_unix_micros: i64,
    pub level_number: Option<i32>,
    pub difficulty_number: Option<i32>,
    pub status: RunStatus,
    pub time_seconds: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct CombinedBestRow {
    pub level_number: i32,
    pub difficulty_number: i32,
    pub time_seconds: i32,
}

#[derive(Debug, Clone)]
pub struct MonitorSessionRow {
    pub session_id: String,
    pub started_unix_micros: i64,
    pub ended_unix_micros: Option<i64>,
    pub source_name: String,
    pub initial_cv_language: Option<String>,
    pub plugin_version: String,
    pub end_reason: Option<String>,
}

pub fn initialise(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(CREATE_TABLE)?;
    conn.execute_batch(CREATE_STATUS_TIMESTAMP_INDEX)?;
    conn.execute_batch(CREATE_LEVEL_DIFFICULTY_TIMESTAMP_INDEX)?;
    conn.execute_batch(CREATE_TIME_INDEX)?;
    conn.execute_batch(CREATE_TIME_SORT_INDEX)?;
    Ok(())
}

pub fn drop_tables(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(DROP_TABLES)?;
    Ok(())
}

pub fn migrate_v2_to_v3(conn: &mut Connection) -> anyhow::Result<()> {
    let tx = conn.transaction()?;
    tx.execute_batch("ALTER TABLE runs RENAME TO runs_v2;")?;
    tx.execute_batch(CREATE_TABLE)?;
    tx.execute_batch(MIGRATE_V2_ROWS)?;
    initialise(&tx)?;
    initialise_sessions(&tx)?;
    super::meta::set_schema_version(&tx)?;
    tx.commit()?;
    Ok(())
}

pub fn initialise_sessions(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(CREATE_SESSION_TABLES)?;
    Ok(())
}

pub fn list_runs(conn: &Connection) -> anyhow::Result<Vec<RunRecord>> {
    list_runs_sorted(conn, RunSort::Newest)
}

pub fn list_runs_sorted(conn: &Connection, sort: RunSort) -> anyhow::Result<Vec<RunRecord>> {
    let sql = match sort {
        RunSort::Newest => LIST_RUNS_NEWEST,
        RunSort::Oldest => LIST_RUNS_OLDEST,
        RunSort::Fastest => LIST_RUNS_FASTEST,
        RunSort::Slowest => LIST_RUNS_SLOWEST,
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], row_to_run)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn recent_runs(conn: &Connection, limit: usize) -> anyhow::Result<Vec<RunRecord>> {
    let mut stmt = conn.prepare(RECENT_RUNS)?;
    let rows = stmt.query_map([limit as i64], row_to_run)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_run(conn: &Connection, run_id: &str) -> anyhow::Result<Option<RunRecord>> {
    conn.query_row(GET_RUN, [run_id], row_to_run).optional().map_err(Into::into)
}

pub fn get_run_by_path(conn: &Connection, path: &Path) -> anyhow::Result<Option<RunRecord>> {
    conn.query_row(GET_RUN_BY_PATH, [path_to_string(&catalog_path(path))], row_to_run).optional().map_err(Into::into)
}

pub fn insert_finalized(
    conn: &Connection,
    run_id: &str,
    completed_unix_micros: i64,
    metadata: &ClipMetadata,
) -> anyhow::Result<()> {
    conn.execute(
        INSERT_FINALIZED,
        params![
            run_id,
            completed_unix_micros,
            normalized_level_number(metadata.level_number),
            metadata.difficulty.as_deref().and_then(crate::ge::difficulty_number),
            metadata.status.as_str(),
            metadata.time_seconds,
            metadata.retention_state,
            metadata.retention_reason,
            serde_json::to_string(metadata)?,
        ],
    )?;
    Ok(())
}

pub fn best_time(conn: &Connection, level_number: i32, difficulty_number: i32) -> anyhow::Result<Option<i32>> {
    Ok(conn.query_row(BEST_TIME, params![level_number, difficulty_number], |row| row.get(0))?)
}

pub fn statistic_facts(conn: &Connection, from: Option<i64>, to: i64) -> anyhow::Result<Vec<RunStatisticFact>> {
    let mut stmt = conn.prepare(STATISTIC_FACTS)?;
    let rows = stmt.query_map(params![from, to], row_to_statistic_fact)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn total_monitor_session_micros(conn: &Connection, from: Option<i64>, to: i64) -> anyhow::Result<i64> {
    Ok(conn.query_row(TOTAL_MONITOR_SESSION_MICROS, params![from, to], |row| row.get(0))?)
}

pub fn combined_best_rows(conn: &Connection) -> anyhow::Result<Vec<CombinedBestRow>> {
    let mut stmt = conn.prepare(COMBINED_BEST_ROWS)?;
    let rows = stmt.query_map([], |row| {
        Ok(CombinedBestRow { level_number: row.get(0)?, difficulty_number: row.get(1)?, time_seconds: row.get(2)? })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn insert_monitor_session(conn: &Connection, session: &MonitorSessionRow) -> anyhow::Result<()> {
    conn.execute(
        INSERT_MONITOR_SESSION,
        params![
            session.session_id,
            session.started_unix_micros,
            session.ended_unix_micros,
            session.source_name,
            session.initial_cv_language,
            session.plugin_version,
            session.end_reason,
        ],
    )?;
    Ok(())
}

pub fn end_monitor_session(
    conn: &Connection,
    session_id: &str,
    ended_unix_micros: Option<i64>,
    reason: &str,
) -> anyhow::Result<()> {
    conn.execute(END_MONITOR_SESSION, params![ended_unix_micros, reason, session_id])?;
    Ok(())
}

pub fn delete_empty_monitor_session(conn: &Connection, session_id: &str) -> anyhow::Result<()> {
    conn.execute(DELETE_EMPTY_MONITOR_SESSION, [session_id])?;
    Ok(())
}

pub fn associate_run_session(conn: &Connection, run_id: &str, session_id: &str) -> anyhow::Result<()> {
    conn.execute("INSERT INTO run_sessions (session_id, run_id) VALUES (?1, ?2)", params![session_id, run_id])?;
    Ok(())
}

pub fn reconcile_monitor_sessions(conn: &Connection) -> anyhow::Result<usize> {
    Ok(conn.execute(RECONCILE_MONITOR_SESSIONS, [])?)
}

pub fn monitor_sessions(conn: &Connection) -> anyhow::Result<Vec<MonitorSessionRow>> {
    let mut stmt = conn.prepare(MONITOR_SESSIONS)?;
    let rows = stmt.query_map([], row_to_monitor_session)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn monitor_session(conn: &Connection, session_id: &str) -> anyhow::Result<Option<MonitorSessionRow>> {
    conn.query_row(MONITOR_SESSION, [session_id], row_to_monitor_session).optional().map_err(Into::into)
}

pub fn monitor_session_facts(conn: &Connection, session_id: &str) -> anyhow::Result<Vec<RunStatisticFact>> {
    let mut stmt = conn.prepare(MONITOR_SESSION_FACTS)?;
    let rows = stmt.query_map([session_id], row_to_statistic_fact)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn row_to_statistic_fact(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunStatisticFact> {
    let status: String = row.get(4)?;
    let status = RunStatus::from_str(&status).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            format!("unknown run status {status}").into(),
        )
    })?;
    Ok(RunStatisticFact {
        run_id: row.get(0)?,
        completed_unix_micros: row.get(1)?,
        level_number: row.get(2)?,
        difficulty_number: row.get(3)?,
        status,
        time_seconds: row.get(5)?,
    })
}

fn row_to_monitor_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<MonitorSessionRow> {
    Ok(MonitorSessionRow {
        session_id: row.get(0)?,
        started_unix_micros: row.get(1)?,
        ended_unix_micros: row.get(2)?,
        source_name: row.get(3)?,
        initial_cv_language: row.get(4)?,
        plugin_version: row.get(5)?,
        end_reason: row.get(6)?,
    })
}

fn normalized_level_number(value: Option<i32>) -> Option<i32> {
    value.filter(|number| (1..=20).contains(number))
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
        ATTACH_SAVED_CLIP,
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
        UPDATE_METADATA,
        params![
            normalized_level_number(metadata.level_number),
            metadata.difficulty.as_deref().and_then(crate::ge::difficulty_number),
            metadata.status.as_str(),
            metadata.time_seconds,
            serde_json::to_string(metadata)?,
            run_id,
        ],
    )?;
    anyhow::ensure!(changed == 1, "run not found while updating metadata");
    Ok(())
}

pub fn update_retention(
    conn: &Connection,
    run_id: &str,
    state: RunRetentionState,
    reason: Option<&str>,
    metadata: &ClipMetadata,
) -> anyhow::Result<()> {
    let changed = conn.execute(
        "UPDATE runs SET retention_state = ?1, retention_reason = ?2, metadata_json = ?3 WHERE run_id = ?4",
        params![state.as_str(), reason, serde_json::to_string(metadata)?, run_id],
    )?;
    anyhow::ensure!(changed == 1, "run not found while updating retention");
    Ok(())
}

pub fn upsert_imported(conn: &Connection, clip: &IndexedRunClip, completed_unix_micros: i64) -> anyhow::Result<()> {
    conn.execute(
        UPSERT_IMPORTED,
        params![
            clip.run_id,
            completed_unix_micros,
            normalized_level_number(clip.metadata.level_number),
            clip.metadata.difficulty.as_deref().and_then(crate::ge::difficulty_number),
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
    conn.execute(DETACH_CLIP, params![state.as_str(), reason, serde_json::to_string(metadata)?, run_id])?;
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
