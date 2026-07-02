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
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::ffmpeg::{self, ClipMetadata};
use crate::http::AppState;
use crate::settings::AppSettings;

const THUMBNAIL_MAX_WIDTH: u32 = 320;

#[derive(Debug, Deserialize)]
pub struct RunPathParams {
    path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunsResponse {
    directories: Vec<RunDirectoryScan>,
    clips: Vec<RunClip>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDirectoryScan {
    kind: RunDirectoryKind,
    path: String,
    exists: bool,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
enum RunPathError {
    BadRequest(&'static str),
    Forbidden(&'static str),
    NotFound(&'static str),
    Probe(anyhow::Error),
}

impl RunPathError {
    fn into_response(self) -> Response {
        match self {
            RunPathError::BadRequest(message) => (StatusCode::BAD_REQUEST, message).into_response(),
            RunPathError::Forbidden(message) => (StatusCode::FORBIDDEN, message).into_response(),
            RunPathError::NotFound(message) => (StatusCode::NOT_FOUND, message).into_response(),
            RunPathError::Probe(err) => {
                tracing::warn!("failed to probe requested run clip: {err:#}");
                (StatusCode::BAD_REQUEST, "could not read run clip metadata").into_response()
            }
        }
    }
}

#[axum::debug_handler]
pub async fn handle_list(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let settings = state.settings.get();
    let response = tokio::task::spawn_blocking(move || list_configured_runs(&settings)).await.map_err(|err| {
        tracing::error!("run listing task failed: {err:#}");
        (StatusCode::INTERNAL_SERVER_ERROR, "run listing failed").into_response()
    })?;

    Ok((StatusCode::OK, Json(response)))
}

pub async fn handle_thumbnail(State(state): State<AppState>, Query(params): Query<RunPathParams>) -> Result<Response> {
    let settings = state.settings.get();
    let path = authorize_tagged_run_path(&settings, &params.path).map_err(RunPathError::into_response)?;
    let bytes = tokio::task::spawn_blocking(move || ffmpeg::thumbnail_bmp(&path, THUMBNAIL_MAX_WIDTH))
        .await
        .map_err(|err| {
            tracing::error!("thumbnail task failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "thumbnail failed").into_response()
        })?
        .map_err(|err| {
            tracing::warn!("failed to create run thumbnail: {err:#}");
            (StatusCode::BAD_REQUEST, "thumbnail failed").into_response()
        })?;

    Ok(([(header::CONTENT_TYPE, "image/bmp")], bytes).into_response())
}

pub async fn handle_video(
    State(state): State<AppState>,
    Query(params): Query<RunPathParams>,
    headers: HeaderMap,
) -> Result<Response> {
    let settings = state.settings.get();
    let path = authorize_tagged_run_path(&settings, &params.path).map_err(RunPathError::into_response)?;
    serve_video_file(path, &headers).await
}

pub fn list_configured_runs(settings: &AppSettings) -> RunsResponse {
    let dirs = configured_run_directories(settings);
    let mut directories = Vec::new();
    let mut clips = Vec::new();

    for dir in dirs {
        let display_path = dir.path.to_string_lossy().into_owned();
        match fs::metadata(&dir.path) {
            Ok(metadata) if metadata.is_dir() => {
                directories.push(RunDirectoryScan { kind: dir.kind, path: display_path, exists: true, error: None });
                match list_tagged_clips_in_directory(&dir.path) {
                    Ok(mut found) => clips.append(&mut found),
                    Err(err) => {
                        tracing::warn!(path = %dir.path.display(), "failed to scan run directory: {err:#}");
                        if let Some(scan) = directories.last_mut() {
                            scan.error = Some(err.to_string());
                        }
                    }
                }
            }
            Ok(_) => directories.push(RunDirectoryScan {
                kind: dir.kind,
                path: display_path,
                exists: false,
                error: Some("configured path is not a directory".to_owned()),
            }),
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

pub fn list_tagged_clips_in_directory(dir: &Path) -> anyhow::Result<Vec<RunClip>> {
    let mut clips = Vec::new();
    for path in video_files_in_directory(dir)? {
        match tagged_clip(&path) {
            Ok(Some(clip)) => clips.push(clip),
            Ok(None) => {}
            Err(err) => tracing::debug!(path = %path.display(), "skipping non-readable run clip candidate: {err:#}"),
        }
    }
    Ok(clips)
}

pub fn video_files_in_directory(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("reading directory {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type().is_ok_and(|file_type| file_type.is_file()) && is_video_file(&path) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
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

fn authorize_tagged_run_path(settings: &AppSettings, raw_path: &str) -> std::result::Result<PathBuf, RunPathError> {
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

    let range = parse_range(headers, len).map_err(|response| response.into_response())?;
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

fn parse_range(headers: &HeaderMap, len: u64) -> std::result::Result<Option<(u64, u64)>, Response> {
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

fn range_not_satisfiable(len: u64) -> Response {
    (
        StatusCode::RANGE_NOT_SATISFIABLE,
        [(header::CONTENT_RANGE, format!("bytes */{len}"))],
        "requested range is not satisfiable",
    )
        .into_response()
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
