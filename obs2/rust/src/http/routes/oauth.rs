use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use serde::Deserialize;

use crate::http::AppState;

#[derive(Deserialize)]
pub struct OAuthQuery {
    code: Option<String>,
}

#[axum::debug_handler]
pub async fn handle_callback(State(state): State<AppState>, Query(query): Query<OAuthQuery>) -> impl IntoResponse {
    if let Some(code) = query.code {
        let mut pending = state.oauth_pending.lock().await;
        if let Some(tx) = pending.take() {
            let _ = tx.send(code);
            Html(concat!(
                "<html>",
                "<head><script>window.close();</script></head>",
                "<body>Authenticated! You can now close this window.</body>",
                "</html>",
            ))
            .into_response()
        } else {
            (StatusCode::BAD_REQUEST, Html("No pending OAuth flow.")).into_response()
        }
    } else {
        (StatusCode::BAD_REQUEST, Html("OAuth2 code not found in request.")).into_response()
    }
}
