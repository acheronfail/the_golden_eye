use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};

pub async fn handler() -> Result<impl IntoResponse> {
    // TODO: get screenshot from obs
    Ok(StatusCode::OK)
}
