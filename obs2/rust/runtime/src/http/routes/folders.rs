use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::Deserialize;

use crate::desktop::folders::{
    FolderPickResponse,
    default_videos_directory,
    initial_directory,
    pick_folder_on_ui_thread,
    validate_folder_path,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderPickRequest {
    title: Option<String>,
    current_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderValidateRequest {
    path: String,
}

#[axum::debug_handler]
pub async fn handle_pick(Json(req): Json<FolderPickRequest>) -> Result<impl IntoResponse> {
    let title = req.title.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "Choose folder".to_owned());
    let start_dir = req.current_path.as_deref().and_then(initial_directory).or_else(default_videos_directory);

    let selected = tokio::task::spawn_blocking(move || pick_folder_on_ui_thread(title, start_dir))
        .await
        .map_err(|err| {
            tracing::error!("folder picker task failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "folder picker failed").into_response()
        })?
        .map_err(|err| {
            tracing::error!("folder picker failed: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "folder picker failed").into_response()
        })?;

    Ok((
        StatusCode::OK,
        Json(FolderPickResponse {
            cancelled: selected.is_none(),
            path: selected.map(|path| path.to_string_lossy().into_owned()),
        }),
    ))
}

#[axum::debug_handler]
pub async fn handle_validate(Json(req): Json<FolderValidateRequest>) -> Result<impl IntoResponse> {
    Ok((StatusCode::OK, Json(validate_folder_path(&req.path))))
}
