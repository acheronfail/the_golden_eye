use axum::Json;
use axum::response::{IntoResponse, Result};

use crate::obs::collect_sources;

pub async fn handler() -> Result<impl IntoResponse> {
    Ok(Json(collect_sources()))
}
