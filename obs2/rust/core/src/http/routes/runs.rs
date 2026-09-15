use std::path::{Path, PathBuf};

use axum::Json;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response, Result};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::app::AppState;
use crate::db::run_catalog::RunCursor;
use crate::run_library::*;

pub(crate) fn run_error_response(error: RunPathError) -> Response {
    match error {
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

#[axum::debug_handler]
pub async fn handle_list(State(state): State<AppState>, Query(params): Query<RunsParams>) -> Result<impl IntoResponse> {
    let settings = state.settings.get_effective();
    let cursor = params
        .cursor
        .as_deref()
        .map(RunCursor::decode)
        .transpose()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid run page cursor").into_response())?;
    let response = tokio::task::spawn_blocking(move || state.runs.list(&settings, params, cursor))
        .await
        .map_err(|err| {
            tracing::error!("run listing task failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "run listing failed").into_response()
        })?
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response())?;

    Ok((StatusCode::OK, Json(response)))
}

#[axum::debug_handler]
pub async fn handle_recent(
    State(state): State<AppState>,
    Query(params): Query<RecentRunsParams>,
) -> Result<impl IntoResponse> {
    let settings = state.settings.get_effective();
    let limit = params
        .limit
        .unwrap_or(crate::run_monitoring::MAX_RECENT_RUN_LIMIT)
        .clamp(1, crate::run_monitoring::MAX_RECENT_RUN_LIMIT);
    let runs = tokio::task::spawn_blocking(move || state.runs.recent(&settings, limit))
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response())?
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response())?;
    Ok(Json(runs))
}

#[axum::debug_handler]
pub async fn handle_keep(State(state): State<AppState>, Json(req): Json<RunIdRequest>) -> Result<impl IntoResponse> {
    let library = state.runs.clone();
    let run = tokio::task::spawn_blocking(move || library.keep(&req.run_id))
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response())?
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()).into_response())?;
    Ok(Json(run))
}

#[axum::debug_handler]
pub async fn handle_delete_run(
    State(state): State<AppState>,
    Json(req): Json<RunDeleteRequest>,
) -> Result<impl IntoResponse> {
    let library = state.runs.clone();
    let retained = tokio::task::spawn_blocking(move || library.delete(&req.run_id, req.keep_history))
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response())?
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()).into_response())?;
    Ok(Json(retained))
}

pub async fn handle_video(
    State(state): State<AppState>,
    Query(params): Query<RunPathParams>,
    headers: HeaderMap,
) -> Result<Response> {
    let settings = state.settings.get_effective();
    let path = authorize_tagged_run_path(&settings, &params.path).map_err(run_error_response)?;
    serve_video_file(path, &headers).await
}

pub async fn handle_rename(
    State(state): State<AppState>,
    Json(req): Json<RunRenameRequest>,
) -> Result<impl IntoResponse> {
    let settings = state.settings.get_effective();
    let catalog = state.run_catalog.clone();
    let clip = tokio::task::spawn_blocking(move || rename_run_clip(&settings, &catalog, req))
        .await
        .map_err(|err| {
            tracing::error!("run rename task failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "run rename failed").into_response()
        })?
        .map_err(run_error_response)?;

    Ok((StatusCode::OK, Json(clip)))
}

#[axum::debug_handler]
pub async fn handle_update_metadata(
    State(state): State<AppState>,
    Json(req): Json<RunMetadataUpdateRequest>,
) -> Result<impl IntoResponse> {
    let library = state.runs.clone();
    let clip = tokio::task::spawn_blocking(move || library.update_metadata(req))
        .await
        .map_err(|err| {
            tracing::error!("run metadata update task failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "run metadata update failed").into_response()
        })?
        .map_err(run_error_response)?;

    Ok((StatusCode::OK, Json(clip)))
}

#[axum::debug_handler]
pub async fn handle_create_manual(
    State(state): State<AppState>,
    Json(req): Json<ManualRunRequest>,
) -> Result<impl IntoResponse> {
    let library = state.runs.clone();
    let run = tokio::task::spawn_blocking(move || library.create_manual(req))
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response())?
        .map_err(run_error_response)?;
    Ok((StatusCode::CREATED, Json(run)))
}

#[axum::debug_handler]
pub async fn handle_import_elite(
    State(state): State<AppState>,
    Json(req): Json<EliteImportRequest>,
) -> Result<impl IntoResponse> {
    let username = req.username.trim().trim_start_matches('~').to_owned();
    let elite_runs = crate::run_library::elite::fetch_history(&req.username)
        .await
        .map_err(|err| (elite_fetch_error_status(&err), err.to_string()).into_response())?;
    let library = state.runs.clone();
    let result = tokio::task::spawn_blocking(move || library.import_elite(&username, elite_runs))
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response())?
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()).into_response())?;
    Ok(Json(result))
}

fn elite_fetch_error_status(error: &anyhow::Error) -> StatusCode {
    if error.downcast_ref::<crate::run_library::elite::UserNotFound>().is_some() {
        StatusCode::NOT_FOUND
    } else {
        StatusCode::BAD_GATEWAY
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

#[cfg(test)]
#[path = "runs_test.rs"]
mod tests;
