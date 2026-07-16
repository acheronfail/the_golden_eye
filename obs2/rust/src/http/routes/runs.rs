use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Context;
use axum::Json;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_util::io::ReaderStream;

use crate::ffmpeg::{self, ClipMetadata};
use crate::http::AppState;
use crate::settings::AppSettings;

#[derive(Debug, Deserialize)]
pub struct RunPathParams {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRenameRequest {
    path: String,
    file_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunMetadataUpdateRequest {
    path: String,
    metadata: EditableRunMetadata,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditableRunMetadata {
    rom_language: String,
    status: String,
    difficulty: String,
    time: String,
    level: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunsResponse {
    directories: Vec<RunDirectoryScan>,
    clips: Vec<RunClip>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RunsStreamEvent {
    Directory { directory: RunDirectoryScan },
    Clip { clip: Box<RunClip> },
    Done,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDirectoryScan {
    kind: RunDirectoryKind,
    path: String,
    exists: bool,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunDirectoryKind {
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunClip {
    path: String,
    file_name: String,
    directory: String,
    size_bytes: u64,
    modified: Option<String>,
    duration_secs: Option<f64>,
    metadata: ClipMetadata,
}

#[derive(Debug, Clone)]
struct ConfiguredRunDirectory {
    kind: RunDirectoryKind,
    path: PathBuf,
}

#[derive(Debug)]
pub(crate) enum RunPathError {
    BadRequest(&'static str),
    Conflict(&'static str),
    Forbidden(&'static str),
    NotFound(&'static str),
    Probe(anyhow::Error),
    Internal(anyhow::Error),
}

impl RunPathError {
    pub(crate) fn into_response(self) -> Response {
        match self {
            RunPathError::BadRequest(message) => (StatusCode::BAD_REQUEST, message).into_response(),
            RunPathError::Conflict(message) => (StatusCode::CONFLICT, message).into_response(),
            RunPathError::Forbidden(message) => (StatusCode::FORBIDDEN, message).into_response(),
            RunPathError::NotFound(message) => (StatusCode::NOT_FOUND, message).into_response(),
            RunPathError::Probe(err) => {
                tracing::warn!("failed to probe requested run clip: {err:#}");
                (StatusCode::BAD_REQUEST, "could not read run clip metadata").into_response()
            }
            RunPathError::Internal(err) => {
                tracing::warn!("run file operation failed: {err:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "run file operation failed").into_response()
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct LevelOption {
    name: &'static str,
    number: i32,
}

const LEVEL_OPTIONS: [LevelOption; 20] = [
    LevelOption { name: "Dam", number: 1 },
    LevelOption { name: "Facility", number: 2 },
    LevelOption { name: "Runway", number: 3 },
    LevelOption { name: "Surface 1", number: 4 },
    LevelOption { name: "Bunker 1", number: 5 },
    LevelOption { name: "Silo", number: 6 },
    LevelOption { name: "Frigate", number: 7 },
    LevelOption { name: "Surface 2", number: 8 },
    LevelOption { name: "Bunker 2", number: 9 },
    LevelOption { name: "Statue", number: 10 },
    LevelOption { name: "Archives", number: 11 },
    LevelOption { name: "Streets", number: 12 },
    LevelOption { name: "Depot", number: 13 },
    LevelOption { name: "Train", number: 14 },
    LevelOption { name: "Jungle", number: 15 },
    LevelOption { name: "Control", number: 16 },
    LevelOption { name: "Caverns", number: 17 },
    LevelOption { name: "Cradle", number: 18 },
    LevelOption { name: "Aztec", number: 19 },
    LevelOption { name: "Egypt", number: 20 },
];

#[axum::debug_handler]
pub async fn handle_list(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let settings = state.settings.get_effective();
    let response = tokio::task::spawn_blocking(move || list_configured_runs(&settings)).await.map_err(|err| {
        tracing::error!("run listing task failed: {err:#}");
        (StatusCode::INTERNAL_SERVER_ERROR, "run listing failed").into_response()
    })?;

    Ok((StatusCode::OK, Json(response)))
}

pub async fn handle_stream(State(state): State<AppState>) -> Result<Response> {
    let settings = state.settings.get_effective();
    let (tx, mut rx) = mpsc::channel::<String>(32);
    let (mut writer, reader) = tokio::io::duplex(64 * 1024);

    std::mem::drop(tokio::task::spawn_blocking(move || {
        stream_configured_runs(&settings, |event| {
            let Ok(mut line) = serde_json::to_string(&event) else {
                return true;
            };
            line.push('\n');
            tx.blocking_send(line).is_ok()
        });
    }));

    std::mem::drop(tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if writer.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    }));

    let stream = ReaderStream::new(reader);
    let body = Body::from_stream(stream);
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .body(body)
        .map_err(|err| {
            tracing::error!("failed to build run stream response: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "run stream response failed").into_response()
        })?;
    Ok(response)
}

pub async fn handle_video(
    State(state): State<AppState>,
    Query(params): Query<RunPathParams>,
    headers: HeaderMap,
) -> Result<Response> {
    let settings = state.settings.get_effective();
    let path = authorize_tagged_run_path(&settings, &params.path).map_err(RunPathError::into_response)?;
    serve_video_file(path, &headers).await
}

#[axum::debug_handler]
pub async fn handle_delete(
    State(state): State<AppState>,
    Query(params): Query<RunPathParams>,
) -> Result<impl IntoResponse> {
    let settings = state.settings.get_effective();
    let path = authorize_tagged_run_path(&settings, &params.path).map_err(RunPathError::into_response)?;

    tokio::task::spawn_blocking(move || fs::remove_file(&path))
        .await
        .map_err(|err| {
            tracing::error!("run delete task failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "run delete failed").into_response()
        })?
        .map_err(|err| RunPathError::Internal(err.into()).into_response())?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn handle_rename(
    State(state): State<AppState>,
    Json(req): Json<RunRenameRequest>,
) -> Result<impl IntoResponse> {
    let settings = state.settings.get_effective();
    let clip = tokio::task::spawn_blocking(move || rename_run_clip(&settings, req))
        .await
        .map_err(|err| {
            tracing::error!("run rename task failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "run rename failed").into_response()
        })?
        .map_err(RunPathError::into_response)?;

    Ok((StatusCode::OK, Json(clip)))
}

#[axum::debug_handler]
pub async fn handle_update_metadata(
    State(state): State<AppState>,
    Json(req): Json<RunMetadataUpdateRequest>,
) -> Result<impl IntoResponse> {
    let settings = state.settings.get_effective();
    let clip = tokio::task::spawn_blocking(move || update_run_metadata(&settings, req))
        .await
        .map_err(|err| {
            tracing::error!("run metadata update task failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "run metadata update failed").into_response()
        })?
        .map_err(RunPathError::into_response)?;

    Ok((StatusCode::OK, Json(clip)))
}

pub fn list_configured_runs(settings: &AppSettings) -> RunsResponse {
    let dirs = configured_run_directories(settings);
    let mut directories = Vec::new();
    let mut clips = Vec::new();
    let mut seen = HashSet::new();

    for dir in dirs {
        let display_path = dir.path.to_string_lossy().into_owned();
        match ensure_configured_run_directory(&dir.path) {
            Ok(()) => {
                directories.push(RunDirectoryScan { kind: dir.kind, path: display_path, exists: true, error: None });
                match list_tagged_clips_in_directory_with_seen(&dir.path, &mut seen) {
                    Ok(mut found) => clips.append(&mut found),
                    Err(err) => {
                        tracing::warn!(path = %dir.path.display(), "failed to scan run directory: {err:#}");
                        if let Some(scan) = directories.last_mut() {
                            scan.error = Some(err.to_string());
                        }
                    }
                }
            }
            Err(err) => directories.push(RunDirectoryScan {
                kind: dir.kind,
                path: display_path,
                exists: false,
                error: Some(err.to_string()),
            }),
        }
    }

    clips.sort_by(|a, b| {
        b.metadata
            .timestamp
            .cmp(&a.metadata.timestamp)
            .then_with(|| b.modified.cmp(&a.modified))
            .then_with(|| b.path.cmp(&a.path))
    });

    RunsResponse { directories, clips }
}

pub fn stream_configured_runs(settings: &AppSettings, mut emit: impl FnMut(RunsStreamEvent) -> bool) {
    let dirs = configured_run_directories(settings);
    let mut seen = HashSet::new();

    for dir in dirs {
        let display_path = dir.path.to_string_lossy().into_owned();
        match ensure_configured_run_directory(&dir.path) {
            Ok(()) => {
                if !emit(RunsStreamEvent::Directory {
                    directory: RunDirectoryScan {
                        kind: dir.kind,
                        path: display_path.clone(),
                        exists: true,
                        error: None,
                    },
                }) {
                    return;
                }
                if let Err(err) = stream_tagged_clips_in_directory(&dir.path, &mut seen, &mut emit) {
                    tracing::warn!(path = %dir.path.display(), "failed to scan run directory: {err:#}");
                    if !emit(RunsStreamEvent::Directory {
                        directory: RunDirectoryScan {
                            kind: dir.kind,
                            path: display_path,
                            exists: true,
                            error: Some(err.to_string()),
                        },
                    }) {
                        return;
                    }
                }
            }
            Err(err) => {
                if !emit(RunsStreamEvent::Directory {
                    directory: RunDirectoryScan {
                        kind: dir.kind,
                        path: display_path,
                        exists: false,
                        error: Some(err.to_string()),
                    },
                }) {
                    return;
                }
            }
        }
    }

    let _ = emit(RunsStreamEvent::Done);
}

pub(crate) fn ensure_configured_run_directory(dir: &Path) -> anyhow::Result<()> {
    match fs::metadata(dir) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => anyhow::bail!("configured path is not a directory"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(dir).with_context(|| format!("creating run directory {}", dir.display()))?;
            let metadata = fs::metadata(dir).with_context(|| format!("checking run directory {}", dir.display()))?;
            if metadata.is_dir() {
                Ok(())
            } else {
                anyhow::bail!("configured path {} exists but is not a directory", dir.display())
            }
        }
        Err(err) => Err(err).with_context(|| format!("reading run directory {}", dir.display())),
    }
}

fn list_tagged_clips_in_directory_with_seen(dir: &Path, seen: &mut HashSet<PathBuf>) -> anyhow::Result<Vec<RunClip>> {
    let mut clips = Vec::new();
    for path in video_files_in_directory_recursive(dir)? {
        if !seen.insert(clip_dedupe_key(&path)) {
            continue;
        }
        match tagged_clip(&path) {
            Ok(Some(clip)) => clips.push(clip),
            Ok(None) => {}
            Err(err) => tracing::debug!(path = %path.display(), "skipping non-readable run clip candidate: {err:#}"),
        }
    }
    Ok(clips)
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

fn stream_tagged_clips_in_directory(
    dir: &Path,
    seen: &mut HashSet<PathBuf>,
    emit: &mut impl FnMut(RunsStreamEvent) -> bool,
) -> anyhow::Result<()> {
    stream_tagged_clips_recursive(dir, seen, emit)
}

fn stream_tagged_clips_recursive(
    dir: &Path,
    seen: &mut HashSet<PathBuf>,
    emit: &mut impl FnMut(RunsStreamEvent) -> bool,
) -> anyhow::Result<()> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("reading directory {}", dir.display()))? {
        entries.push(entry?);
    }
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            stream_tagged_clips_recursive(&path, seen, emit)?;
        } else if file_type.is_file() && is_video_file(&path) {
            if !seen.insert(clip_dedupe_key(&path)) {
                continue;
            }
            match tagged_clip(&path) {
                Ok(Some(clip)) => {
                    if !emit(RunsStreamEvent::Clip { clip: Box::new(clip) }) {
                        return Ok(());
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    tracing::debug!(path = %path.display(), "skipping non-readable run clip candidate: {err:#}")
                }
            }
        }
    }
    Ok(())
}

fn clip_dedupe_key(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn tagged_clip(path: &Path) -> anyhow::Result<Option<RunClip>> {
    let Some(metadata) = ffmpeg::read_clip_metadata(path)? else {
        return Ok(None);
    };
    let fs_metadata = fs::metadata(path).with_context(|| format!("reading metadata for {}", path.display()))?;
    let duration_secs = ffmpeg::duration_secs(path).ok();

    Ok(Some(RunClip {
        path: path.to_string_lossy().into_owned(),
        file_name: path.file_name().and_then(|name| name.to_str()).unwrap_or("clip").to_owned(),
        directory: path.parent().unwrap_or_else(|| Path::new("")).to_string_lossy().into_owned(),
        size_bytes: fs_metadata.len(),
        modified: fs_metadata.modified().ok().map(format_unix_timestamp),
        duration_secs,
        metadata,
    }))
}

pub(crate) fn authorize_tagged_run_path(
    settings: &AppSettings,
    raw_path: &str,
) -> std::result::Result<PathBuf, RunPathError> {
    let requested = resolve_path(raw_path.trim());
    if raw_path.trim().is_empty() {
        return Err(RunPathError::BadRequest("path is required"));
    }
    if !is_video_file(&requested) {
        return Err(RunPathError::BadRequest("path is not a supported video file"));
    }

    let path = fs::canonicalize(&requested).map_err(|_| RunPathError::NotFound("run clip was not found"))?;
    if !configured_run_directories(settings)
        .into_iter()
        .filter_map(|dir| fs::canonicalize(dir.path).ok())
        .any(|dir| path.starts_with(dir))
    {
        return Err(RunPathError::Forbidden("run clip is not in a configured run directory"));
    }

    match ffmpeg::read_clip_metadata(&path) {
        Ok(Some(_)) => Ok(path),
        Ok(None) => Err(RunPathError::Forbidden("run clip was not created by The Golden Eye")),
        Err(err) => Err(RunPathError::Probe(err)),
    }
}

pub(crate) fn configured_run_directory_for_kind(
    settings: &AppSettings,
    kind: RunDirectoryKind,
) -> std::result::Result<PathBuf, RunPathError> {
    match kind {
        RunDirectoryKind::Completed => configured_dir(&settings.completed_output_path)
            .ok_or(RunPathError::NotFound("completed run clip folder is not configured")),
        RunDirectoryKind::Failed => {
            if !settings.save_failed_runs {
                Err(RunPathError::NotFound("failed run clip folder is not configured"))
            } else {
                configured_dir(&settings.failed_output_path)
                    .ok_or(RunPathError::NotFound("failed run clip folder is not configured"))
            }
        }
    }
}

fn rename_run_clip(settings: &AppSettings, req: RunRenameRequest) -> std::result::Result<RunClip, RunPathError> {
    let path = authorize_tagged_run_path(settings, &req.path)?;
    let file_name = normalized_run_file_name(&path, &req.file_name)?;
    let parent = path.parent().ok_or(RunPathError::BadRequest("run clip has no parent directory"))?;
    let target = parent.join(file_name);

    if target == path {
        return tagged_clip(&path)?.ok_or(RunPathError::Forbidden("run clip was not created by The Golden Eye"));
    }
    if target.exists() {
        return Err(RunPathError::Conflict("a run clip with that filename already exists"));
    }

    fs::rename(&path, &target).with_context(|| format!("renaming {} to {}", path.display(), target.display()))?;
    tagged_clip(&target)?.ok_or(RunPathError::Forbidden("run clip was not created by The Golden Eye"))
}

fn update_run_metadata(
    settings: &AppSettings,
    req: RunMetadataUpdateRequest,
) -> std::result::Result<RunClip, RunPathError> {
    let path = authorize_tagged_run_path(settings, &req.path)?;
    let mut metadata = ffmpeg::read_clip_metadata(&path)
        .map_err(RunPathError::Probe)?
        .ok_or(RunPathError::Forbidden("run clip was not created by The Golden Eye"))?;

    apply_metadata_update(&mut metadata, req.metadata)?;
    ffmpeg::rewrite_metadata_in_place(&path, &metadata).map_err(RunPathError::Internal)?;

    tagged_clip(&path)?.ok_or(RunPathError::Forbidden("run clip was not created by The Golden Eye"))
}

impl From<anyhow::Error> for RunPathError {
    fn from(err: anyhow::Error) -> Self {
        RunPathError::Internal(err)
    }
}

fn apply_metadata_update(
    metadata: &mut ClipMetadata,
    update: EditableRunMetadata,
) -> std::result::Result<(), RunPathError> {
    let level = normalize_level(&update.level)?;
    let time = normalize_time(&update.time)?;

    metadata.rom_language = normalize_rom_language(&update.rom_language)?.to_owned();
    metadata.status = normalize_status(&update.status)?.to_owned();
    metadata.difficulty = Some(normalize_difficulty(&update.difficulty)?.to_owned());
    metadata.level = level.name.to_owned();
    metadata.level_number = Some(level.number);
    metadata.time = time.as_ref().map(|(_, formatted)| formatted.clone());
    metadata.time_seconds = time.map(|(seconds, _)| seconds);

    Ok(())
}

fn normalize_rom_language(value: &str) -> std::result::Result<&'static str, RunPathError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "en" => Ok("en"),
        "jp" => Ok("jp"),
        _ => Err(RunPathError::BadRequest("rom language must be en or jp")),
    }
}

fn normalize_status(value: &str) -> std::result::Result<&'static str, RunPathError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "complete" | "completed" => Ok("complete"),
        "failed" => Ok("failed"),
        "abort" | "aborted" => Ok("abort"),
        "kia" | "killed in action" => Ok("kia"),
        _ => Err(RunPathError::BadRequest("status must be failed, aborted, completed, or killed in action")),
    }
}

fn normalize_difficulty(value: &str) -> std::result::Result<&'static str, RunPathError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "agent" => Ok("Agent"),
        "secret agent" => Ok("Secret Agent"),
        "00 agent" => Ok("00 Agent"),
        "007" => Ok("007"),
        _ => Err(RunPathError::BadRequest("difficulty must be agent, secret agent, 00 agent, or 007")),
    }
}

fn normalize_level(value: &str) -> std::result::Result<LevelOption, RunPathError> {
    let trimmed = value.trim();
    LEVEL_OPTIONS
        .iter()
        .copied()
        .find(|level| level.name.eq_ignore_ascii_case(trimmed))
        .ok_or(RunPathError::BadRequest("level must be one of the supported GoldenEye levels"))
}

fn normalize_time(value: &str) -> std::result::Result<Option<(i32, String)>, RunPathError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let Some((minutes, seconds)) = trimmed.split_once(':') else {
        return Err(RunPathError::BadRequest("time must be formatted as mm:ss"));
    };
    if minutes.is_empty() || seconds.len() != 2 || !minutes.chars().all(|c| c.is_ascii_digit()) {
        return Err(RunPathError::BadRequest("time must be formatted as mm:ss"));
    }
    let minutes = minutes.parse::<i32>().map_err(|_| RunPathError::BadRequest("time minutes are invalid"))?;
    let seconds = seconds.parse::<i32>().map_err(|_| RunPathError::BadRequest("time seconds are invalid"))?;
    if !(0..=59).contains(&seconds) {
        return Err(RunPathError::BadRequest("time seconds must be between 00 and 59"));
    }

    let total = minutes
        .checked_mul(60)
        .and_then(|m| m.checked_add(seconds))
        .ok_or(RunPathError::BadRequest("time is too large"))?;

    Ok(Some((total, format!("{minutes:02}:{seconds:02}"))))
}

fn normalized_run_file_name(path: &Path, raw: &str) -> std::result::Result<String, RunPathError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(RunPathError::BadRequest("filename is required"));
    }
    if trimmed == "." || trimmed == ".." || trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains('\0') {
        return Err(RunPathError::BadRequest("filename cannot contain path separators"));
    }

    let mut file_name = trimmed.to_owned();
    if Path::new(&file_name).extension().is_none()
        && let Some(ext) = path.extension().and_then(|ext| ext.to_str())
        && !ext.is_empty()
    {
        file_name.push('.');
        file_name.push_str(ext);
    }
    if !is_video_file(Path::new(&file_name)) {
        return Err(RunPathError::BadRequest("filename must use a supported video extension"));
    }

    Ok(file_name)
}

async fn serve_video_file(path: PathBuf, headers: &HeaderMap) -> Result<Response> {
    let mut file = tokio::fs::File::open(&path).await.map_err(|err| {
        tracing::warn!(path = %path.display(), "failed to open run video: {err}");
        (StatusCode::NOT_FOUND, "run video was not found").into_response()
    })?;
    let len = file
        .metadata()
        .await
        .map_err(|err| {
            tracing::warn!(path = %path.display(), "failed to read run video metadata: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "run video metadata failed").into_response()
        })?
        .len();

    let range = parse_range(headers, len).map_err(|response| *response)?;
    let (status, start, end) = match range {
        Some((start, end)) => (StatusCode::PARTIAL_CONTENT, start, end),
        None => (StatusCode::OK, 0, len.saturating_sub(1)),
    };
    let content_len = if len == 0 { 0 } else { end - start + 1 };

    if start > 0 {
        file.seek(std::io::SeekFrom::Start(start)).await.map_err(|err| {
            tracing::warn!(path = %path.display(), start, "failed to seek run video: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "run video seek failed").into_response()
        })?;
    }

    let stream = ReaderStream::new(file.take(content_len));
    let body = Body::from_stream(stream);
    let mut response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime_for_path(&path))
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, content_len.to_string())
        .body(body)
        .map_err(|err| {
            tracing::error!("failed to build run video response: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "run video response failed").into_response()
        })?;

    if status == StatusCode::PARTIAL_CONTENT {
        response.headers_mut().insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {start}-{end}/{len}"))
                .unwrap_or_else(|_| HeaderValue::from_static("bytes */*")),
        );
    }

    Ok(response)
}

fn parse_range(headers: &HeaderMap, len: u64) -> std::result::Result<Option<(u64, u64)>, Box<Response>> {
    let Some(range) = headers.get(header::RANGE) else {
        return Ok(None);
    };
    if len == 0 {
        return Err(range_not_satisfiable(len));
    }

    let range = range.to_str().map_err(|_| range_not_satisfiable(len))?;
    let spec = range.strip_prefix("bytes=").ok_or_else(|| range_not_satisfiable(len))?;
    if spec.contains(',') {
        return Err(range_not_satisfiable(len));
    }

    let (start, end) = if let Some(suffix) = spec.strip_prefix('-') {
        let suffix_len = suffix.parse::<u64>().map_err(|_| range_not_satisfiable(len))?;
        if suffix_len == 0 {
            return Err(range_not_satisfiable(len));
        }
        (len.saturating_sub(suffix_len), len - 1)
    } else {
        let (start, end) = spec.split_once('-').ok_or_else(|| range_not_satisfiable(len))?;
        let start = start.parse::<u64>().map_err(|_| range_not_satisfiable(len))?;
        let end = if end.is_empty() { len - 1 } else { end.parse::<u64>().map_err(|_| range_not_satisfiable(len))? };
        (start, end.min(len - 1))
    };

    if start > end || start >= len {
        return Err(range_not_satisfiable(len));
    }

    Ok(Some((start, end)))
}

fn range_not_satisfiable(len: u64) -> Box<Response> {
    Box::new(
        (
            StatusCode::RANGE_NOT_SATISFIABLE,
            [(header::CONTENT_RANGE, format!("bytes */{len}"))],
            "requested range is not satisfiable",
        )
            .into_response(),
    )
}

fn configured_run_directories(settings: &AppSettings) -> Vec<ConfiguredRunDirectory> {
    let mut dirs = Vec::new();
    if let Some(path) = configured_dir(&settings.completed_output_path) {
        dirs.push(ConfiguredRunDirectory { kind: RunDirectoryKind::Completed, path });
    }
    if settings.save_failed_runs
        && let Some(path) = configured_dir(&settings.failed_output_path)
        && !dirs.iter().any(|dir| dir.path == path)
    {
        dirs.push(ConfiguredRunDirectory { kind: RunDirectoryKind::Failed, path });
    }
    dirs
}

fn configured_dir(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(resolve_path(trimmed))
}

fn resolve_path(path: &str) -> PathBuf {
    let expanded = expand_home(path);
    if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(expanded)
    }
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~"
        && let Some(home) = home_dir()
    {
        return home;
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

fn is_video_file(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
        matches!(
            ext.to_ascii_lowercase().as_str(),
            "mp4" | "mov" | "m4v" | "mkv" | "webm" | "flv" | "ts" | "avi" | "mpg" | "mpeg"
        )
    })
}

fn mime_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_ascii_lowercase()) {
        Some(ext) if ext == "mp4" || ext == "m4v" => "video/mp4",
        Some(ext) if ext == "mov" => "video/quicktime",
        Some(ext) if ext == "mkv" => "video/x-matroska",
        Some(ext) if ext == "webm" => "video/webm",
        Some(ext) if ext == "flv" => "video/x-flv",
        Some(ext) if ext == "ts" => "video/mp2t",
        Some(ext) if ext == "avi" => "video/x-msvideo",
        Some(ext) if ext == "mpg" || ext == "mpeg" => "video/mpeg",
        _ => "application/octet-stream",
    }
}

fn format_unix_timestamp(time: SystemTime) -> String {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(err) => format!("-{}", err.duration().as_secs()),
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::UNIX_EPOCH;

    use super::*;

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(label: &str) -> Self {
            loop {
                let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
                let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
                let path = std::env::temp_dir().join(format!("ge-runs-{label}-{}-{nanos}-{id}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => return TestDir { path },
                    Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(err) => panic!("failed to create test dir {}: {err}", path.display()),
                }
            }
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

    #[test]
    fn normalize_time_formats_mm_ss_and_seconds() {
        assert_eq!(normalize_time("1:02").unwrap(), Some((62, "01:02".to_owned())));
        assert_eq!(normalize_time("12:34").unwrap(), Some((754, "12:34".to_owned())));
        assert_eq!(normalize_time(" ").unwrap(), None);
    }

    #[test]
    fn normalize_time_rejects_bad_values() {
        assert!(matches!(normalize_time("1"), Err(RunPathError::BadRequest(_))));
        assert!(matches!(normalize_time("1:2"), Err(RunPathError::BadRequest(_))));
        assert!(matches!(normalize_time("1:60"), Err(RunPathError::BadRequest(_))));
    }

    #[test]
    fn normalized_run_file_name_preserves_extension_when_missing() {
        let path = Path::new("/runs/original.mov");
        assert_eq!(normalized_run_file_name(path, "renamed").unwrap(), "renamed.mov");
        assert_eq!(normalized_run_file_name(path, "renamed.mp4").unwrap(), "renamed.mp4");
    }

    #[test]
    fn normalized_run_file_name_rejects_paths_and_non_video_extensions() {
        let path = Path::new("/runs/original.mov");
        assert!(matches!(normalized_run_file_name(path, "../renamed.mov"), Err(RunPathError::BadRequest(_))));
        assert!(matches!(normalized_run_file_name(path, "renamed.txt"), Err(RunPathError::BadRequest(_))));
    }

    #[test]
    fn video_files_in_directory_searches_recursively() {
        let dir = TestDir::new("recursive-video-files");
        let nested = dir.join("Surface 2/00 Agent");
        fs::create_dir_all(&nested).unwrap();
        let root_clip = dir.join("root.mov");
        let nested_clip = nested.join("02-03.mp4");
        let ignored = nested.join("notes.txt");
        fs::write(&root_clip, b"root").unwrap();
        fs::write(&nested_clip, b"nested").unwrap();
        fs::write(&ignored, b"ignored").unwrap();

        let files = video_files_in_directory_recursive(&dir.path).unwrap();

        let mut expected = vec![root_clip, nested_clip];
        expected.sort();
        assert_eq!(files, expected);
    }

    #[test]
    fn list_configured_runs_creates_missing_output_directories_before_scanning() {
        let dir = TestDir::new("configured-missing");
        let completed = dir.join("completed/deeply/nested");
        let failed = dir.join("failed/deeply/nested");
        let settings = AppSettings {
            completed_output_path: completed.to_string_lossy().into_owned(),
            save_failed_runs: true,
            failed_output_path: failed.to_string_lossy().into_owned(),
            ..AppSettings::default()
        };

        let runs = list_configured_runs(&settings);

        assert!(completed.is_dir());
        assert!(failed.is_dir());
        assert!(runs.clips.is_empty());
        assert_eq!(runs.directories.len(), 2);
        assert_eq!(runs.directories[0].kind, RunDirectoryKind::Completed);
        assert_eq!(runs.directories[0].path, completed.to_string_lossy());
        assert!(runs.directories[0].exists);
        assert_eq!(runs.directories[0].error, None);
        assert_eq!(runs.directories[1].kind, RunDirectoryKind::Failed);
        assert_eq!(runs.directories[1].path, failed.to_string_lossy());
        assert!(runs.directories[1].exists);
        assert_eq!(runs.directories[1].error, None);
    }
}
