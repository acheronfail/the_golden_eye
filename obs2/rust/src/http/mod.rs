mod routes;
mod state;

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::error_handling::HandleErrorLayer;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{get, post};
pub(crate) use routes::monitor::stop_monitor;
pub use routes::record::ReplayBufferStatus;
use tokio::net::{TcpListener, TcpSocket};
use tokio::sync::oneshot;
use tower::ServiceBuilder;
use tower_http::BoxError;

const API_REQUEST_TIMEOUT: Duration = Duration::from_secs(20 * 60);

pub use state::*;
pub const SERVER_PORT: u16 = 31337;
pub const OAUTH_CALLBACK_PATH: &str = "/oauth/callback";

pub fn collect_sources() -> Vec<routes::sources::Source> {
    routes::sources::collect_sources()
}

pub fn current_replay_buffer_status() -> routes::record::ReplayBufferStatus {
    routes::record::current_replay_buffer_status()
}

/// Logs each request as it arrives and again once a response is produced.
async fn log_requests(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    tracing::debug!(%method, %path, "request received");

    let start = std::time::Instant::now();
    let response = next.run(req).await;
    let elapsed = start.elapsed();

    let status = response.status();
    tracing::debug!(%method, %path, %status, ?elapsed, "request sent");
    response
}

/// Binds the listening socket synchronously so callers (e.g. `ge_rust_start`)
/// learn immediately whether the port bound. Must run inside a `runtime.enter()`
/// guard; sets `SO_REUSEADDR` so the port can rebind across a reload (TIME_WAIT).
pub fn bind_listener() -> std::io::Result<TcpListener> {
    let addr = SocketAddr::from(([0, 0, 0, 0], SERVER_PORT));
    let socket = TcpSocket::new_v4()?;
    socket.set_reuseaddr(true)?;
    socket.bind(addr)?;
    socket.listen(1024)
}

pub async fn serve(listener: TcpListener, shutdown: oneshot::Receiver<()>, state: AppState) -> anyhow::Result<()> {
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
        // A native folder picker can legitimately stay open while the user
        // navigates the OS dialog; keep the local API timeout above that path.
        .timeout(API_REQUEST_TIMEOUT);

    // Build application router

    // NOTE: axum composes middleware from bottom to top; i.e., the last added is the first to be run
    let app = Router::new()
        .route("/api/v1/record/start", post(routes::record::handle_start))
        .route("/api/v1/record/stop", post(routes::record::handle_stop))
        .route("/api/v1/replay-buffer/status", get(routes::record::handle_replay_status))
        .route("/api/v1/monitor/start", post(routes::monitor::handle_start))
        .route("/api/v1/monitor/stop", post(routes::monitor::handle_stop))
        .route("/api/v1/monitor/ws", get(routes::monitor::handle_ws))
        .route("/api/v1/settings", get(routes::settings::handle_get).put(routes::settings::handle_put))
        .route("/api/v1/settings/status", get(routes::settings::handle_status))
        .route("/api/v1/settings/reset", post(routes::settings::handle_reset))
        .route("/api/v1/folders/pick", post(routes::folders::handle_pick))
        .route("/api/v1/folders/validate", post(routes::folders::handle_validate))
        .route("/api/v1/files/reveal", post(routes::files::handle_reveal))
        .route("/api/v1/updates/open", post(routes::updates::handle_open))
        .route("/api/v1/updates/check", post(routes::updates::handle_check_now))
        .route("/api/v1/updates/download", post(routes::updates::handle_download_now))
        .route("/api/v1/updates/status", get(routes::updates::handle_status))
        .route("/api/v1/updates/apply", post(routes::updates::handle_apply_now))
        .route("/api/v1/youtube/status", get(routes::youtube::handle_status))
        .route("/api/v1/youtube/connect", post(routes::youtube::handle_connect))
        .route("/api/v1/youtube/cancel", post(routes::youtube::handle_cancel))
        .route("/api/v1/youtube/disconnect", post(routes::youtube::handle_disconnect))
        .route("/api/v1/youtube/open", post(routes::youtube::handle_open))
        .route("/api/v1/youtube/forget", post(routes::youtube::handle_forget))
        .route("/api/v1/youtube/upload", post(routes::youtube::handle_upload))
        .route(
            "/api/v1/runs",
            get(routes::runs::handle_list)
                .delete(routes::runs::handle_delete)
                .patch(routes::runs::handle_update_metadata),
        )
        .route("/api/v1/runs/stream", get(routes::runs::handle_stream))
        .route("/api/v1/runs/rename", post(routes::runs::handle_rename))
        .route("/api/v1/runs/video", get(routes::runs::handle_video))
        .route("/api/v1/sources", get(routes::sources::handler))
        .route("/api/v1/screenshot", get(routes::screenshot::handler))
        .route("/api/v1/match", post(routes::matcher::handler))
        .route(
            "/api/v1/match/upload",
            // Raise the body limit above Axum's 2 MB default for full-res frames.
            post(routes::matcher::handle_upload).layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route("/api/v1/match/annotations", post(routes::matcher::handle_annotations))
        .route("/api/v1/monitor/frame-dump", post(routes::monitor::handle_frame_dump))
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

    tracing::info!("listening on {}", listener.local_addr()?);
    let _ = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            // Resolve when a shutdown is requested, or if the sender is dropped.
            let _ = shutdown.await;
        })
        .await;
    Ok(())
}
