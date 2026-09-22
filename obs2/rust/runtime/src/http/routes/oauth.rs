use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use serde::Deserialize;

use crate::app::AppState;

#[derive(Deserialize)]
pub struct OAuthQuery {
    code: Option<String>,
    state: Option<String>,
}

#[axum::debug_handler]
pub async fn handle_callback(State(state): State<AppState>, Query(query): Query<OAuthQuery>) -> impl IntoResponse {
    match state.youtube.accept_oauth_callback(query.code, query.state).await {
        Ok(()) => Html(oauth_page(
            "Authorisation complete",
            "Authorisation was completed successfully. You can now close this page and return to The Golden Eye.",
            true,
        ))
        .into_response(),
        Err(error) => {
            use crate::youtube_uploads::CallbackError;
            let message = match error {
                CallbackError::MissingCode => "OAuth code was not found in the request.",
                CallbackError::NoPendingFlow => "No pending OAuth flow was found.",
                CallbackError::StateMismatch => "OAuth state did not match.",
            };
            oauth_error(StatusCode::BAD_REQUEST, message)
        }
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
