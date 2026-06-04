use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Result};

use crate::http::AppState;

// The bundled web app, built by cmake.
const APP_BUNDLE_HTML: &str = include_str!(env!("BROWSER_BUNDLE"));

#[axum::debug_handler]
pub async fn handler(State(_): State<AppState>) -> Result<impl IntoResponse> {
    Ok(([(header::CONTENT_TYPE, "text/html")], APP_BUNDLE_HTML))
}
