mod routes;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::error_handling::HandleErrorLayer;
use axum::http::StatusCode;
use axum::routing::{get, post};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, oneshot};
use tower::ServiceBuilder;
use tower_http::BoxError;

pub struct AppStateInner {
    /// Holds the sender end of a one-shot channel while an OAuth flow is in
    /// progress. The `/oauth/callback` route fires it when the code arrives.
    pub oauth_pending: Mutex<Option<oneshot::Sender<String>>>,
}

pub type AppState = Arc<AppStateInner>;

pub const SERVER_PORT: u16 = 1337;
pub const OAUTH_CALLBACK_PATH: &str = "/oauth/callback";

pub async fn create_server(shutdown: oneshot::Receiver<()>, state: AppState) -> anyhow::Result<()> {

    // Build middleware stack

    // NOTE: tower composes middleware from top to bottom; i.e., the first added is the first to be run
    let middleware = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|error: BoxError| async move {
            if error.is::<tower::timeout::error::Elapsed>() {
                Ok(StatusCode::REQUEST_TIMEOUT)
            } else {
                Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Unhandled internal error: {error}")))
            }
        }))
        .timeout(Duration::from_secs(30));

    // Build application router

    // NOTE: axum composes middleware from bottom to top; i.e., the last added is the first to be run
    let app = Router::new()
        .route("/api/v1/record/start", post(routes::record::handle_start))
        .route("/api/v1/record/stop", post(routes::record::handle_stop))
        .route("/api/v1/sources", get(routes::sources::handler))
        .route("/api/v1/screenshot", get(routes::screenshot::handler))
        .route(OAUTH_CALLBACK_PATH, get(routes::oauth::handle_callback))
        .layer(middleware.into_inner())
        .with_state(state.clone());

    let listener = TcpListener::bind(format!("0.0.0.0:{SERVER_PORT}")).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    let _ = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            // Resolve when a shutdown is requested, or if the sender is dropped.
            let _ = shutdown.await;
        })
        .await;
    Ok(())
}
