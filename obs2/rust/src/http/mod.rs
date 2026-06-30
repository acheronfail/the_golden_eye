mod routes;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::error_handling::HandleErrorLayer;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{get, post};
use tokio::net::TcpSocket;
use tokio::sync::{Mutex, oneshot};
use tokio::sync::watch;
use tower::ServiceBuilder;
use tower_http::BoxError;

use crate::config::Config;
use crate::cv::LevelMatch;

pub struct AppStateInner {
    /// Holds the sender end of a one-shot channel while an OAuth flow is in
    /// progress. The `/oauth/callback` route fires it when the code arrives.
    pub oauth_pending: Mutex<Option<oneshot::Sender<String>>>,
    /// The Discord "now streaming" message posted when a stream starts, kept so
    /// the stop handler can edit it in place rather than posting a new message.
    pub stream_message: Mutex<Option<StreamMessage>>,
    /// The currently running monitor, if any. Enforces a single monitor at a
    /// time: `/api/v1/monitor/start` fails while this is `Some`.
    pub monitor: std::sync::Mutex<Option<routes::monitor::MonitorHandle>>,
    /// Latest `LevelMatch` from the running monitor, broadcast to connected
    /// WebSocket clients. `None` when no monitor is running (set on stop). The
    /// monitor worker only sends a new value when the matched state changes
    /// (ignoring `runtime_ms`), so subscribers aren't flooded with duplicates.
    /// `watch` retains the latest value, so a client connecting mid-run sees the
    /// current match immediately.
    pub match_tx: watch::Sender<Option<LevelMatch>>,
    /// Application configuration, resolved from the environment at startup.
    pub config: Config,
}

/// A Discord webhook message we posted and may later edit.
pub struct StreamMessage {
    pub id: String,
    pub broadcast_url: String,
}

pub type AppState = Arc<AppStateInner>;

pub const SERVER_PORT: u16 = 31337;
pub const OAUTH_CALLBACK_PATH: &str = "/oauth/callback";

/// Logs each request as it arrives and again once a response is produced.
async fn log_requests(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    tracing::info!(%method, %path, "request received");

    let start = std::time::Instant::now();
    let response = next.run(req).await;
    let elapsed = start.elapsed();

    let status = response.status();
    tracing::info!(%method, %path, %status, ?elapsed, "request sent");
    response
}

pub async fn create_server(shutdown: oneshot::Receiver<()>, state: AppState) -> anyhow::Result<()> {
    // Build middleware stack

    // NOTE: tower composes middleware from top to bottom; i.e., the first added is the first to be run
    let middleware = ServiceBuilder::new()
        // Added first so it's outermost: logs every request and sees the final
        // status, including timeouts handled by the layers below.
        .layer(axum::middleware::from_fn(log_requests))
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
        .route("/api/v1/monitor/start", post(routes::monitor::handle_start))
        .route("/api/v1/monitor/stop", post(routes::monitor::handle_stop))
        .route("/api/v1/monitor/status", get(routes::monitor::handle_status))
        .route("/api/v1/monitor/ws", get(routes::monitor::handle_ws))
        .route("/api/v1/sources", get(routes::sources::handler))
        .route("/api/v1/screenshot", get(routes::screenshot::handler))
        .route("/api/v1/match", post(routes::matcher::handler))
        .route(OAUTH_CALLBACK_PATH, get(routes::oauth::handle_callback))
        .route("/", get(routes::index::handler))
        // fallback for frontend spa
        .fallback(get(routes::index::handler))
        .layer(middleware.into_inner());

    // In dev the SPA is served by the Vite dev server on a different origin, so
    // its fetches to this API are cross-origin. Allow them with permissive CORS.
    // Only compiled in for dev builds (CMake BROWSER_DEV=ON) — never in release.
    #[cfg(feature = "dev")]
    let app = app.layer(tower_http::cors::CorsLayer::permissive());

    let app = app.with_state(state.clone());

    // Build the listener with SO_REUSEADDR so we can rebind the port immediately
    // after a previous server instance is torn down — without it, a client socket
    // lingering in TIME_WAIT makes the bind fail with "address already in use",
    // which is exactly what happens on a dev hot reload (stop server, start a new
    // one on the same port).
    let addr: SocketAddr = format!("0.0.0.0:{SERVER_PORT}").parse()?;
    let socket = TcpSocket::new_v4()?;
    socket.set_reuseaddr(true)?;
    socket.bind(addr)?;
    let listener = socket.listen(1024)?;
    tracing::info!("listening on {}", listener.local_addr()?);
    let _ = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            // Resolve when a shutdown is requested, or if the sender is dropped.
            let _ = shutdown.await;
        })
        .await;
    Ok(())
}
