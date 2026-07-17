use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use serde::Deserialize;

use crate::http::AppState;

#[derive(Deserialize)]
pub struct OAuthQuery {
    code: Option<String>,
    state: Option<String>,
}

#[axum::debug_handler]
pub async fn handle_callback(State(state): State<AppState>, Query(query): Query<OAuthQuery>) -> impl IntoResponse {
    if let Some(code) = query.code {
        let mut pending = state.oauth_pending.lock().await;
        if let Some(pending_oauth) = pending.take() {
            if query.state.as_deref() != Some(pending_oauth.state.as_str()) {
                return oauth_error(StatusCode::BAD_REQUEST, "OAuth state did not match.");
            }
            let _ = pending_oauth.tx.send(code);
            Html(oauth_page(
                "Authorisation complete",
                "Authorisation was completed successfully. You can now close this page and return to The Golden Eye.",
                true,
            ))
            .into_response()
        } else {
            oauth_error(StatusCode::BAD_REQUEST, "No pending OAuth flow was found.")
        }
    } else {
        oauth_error(StatusCode::BAD_REQUEST, "OAuth code was not found in the request.")
    }
}

fn oauth_error(status: StatusCode, message: &'static str) -> axum::response::Response {
    (status, Html(oauth_page("Authorisation failed", message, false))).into_response()
}

fn oauth_page(title: &'static str, message: &'static str, close_window: bool) -> String {
    let close_script = if close_window { "<script>setTimeout(() => window.close(), 750);</script>" } else { "" };
    include_str!("../../../templates/oauth_callback.html")
        .replace("{{title}}", title)
        .replace("{{message}}", message)
        .replace("{{close_script}}", close_script)
}
