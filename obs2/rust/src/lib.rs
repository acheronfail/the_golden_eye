mod http;
mod ffi;
mod stream_notifier;

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tracing_subscriber::EnvFilter;

use http::{AppState, AppStateInner};

/// Holds the tokio runtime that is driving the HTTP server, along with the
/// signal used to ask the server to shut down gracefully.
struct ServerHandle {
    runtime: Runtime,
    /// A cloneable handle into the runtime, used to spawn tasks from
    /// synchronous FFI functions without blocking.
    runtime_handle: tokio::runtime::Handle,
    shutdown: oneshot::Sender<()>,
    state: AppState,
}

/// Global handle to the running server. `None` when the server is stopped.
static SERVER: Mutex<Option<ServerHandle>> = Mutex::new(None);

/// Start the HTTP server on a background tokio runtime. Returns immediately
/// without blocking the calling (C) thread. Calling this while the server is
/// already running is a no-op.
#[unsafe(no_mangle)]
pub extern "C" fn ge_rust_start() {
    // setup logging
    {
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!(
                    "{}={level},tower_http={level}",
                    env!("CARGO_CRATE_NAME"),
                    level = if cfg!(debug_assertions) { "debug" } else { "info" }
                )
                .into()
            }));

        subscriber.init();
    }

    let mut guard = match SERVER.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    if guard.is_some() {
        tracing::warn!("ge_rust_start called while server is already running");
        return;
    }

    let runtime = match Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!("failed to create tokio runtime: {error}");
            return;
        }
    };

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let state = Arc::new(AppStateInner {
        oauth_pending: tokio::sync::Mutex::new(None),
        stream_message: tokio::sync::Mutex::new(None),
    });

    // Spawn the server onto the runtime. `spawn` returns immediately so the
    // C caller is never blocked; the runtime drives the future on its own
    // worker threads.
    let state_clone = state.clone();
    runtime.spawn(async move {
        if let Err(error) = http::create_server(shutdown_rx, state_clone).await {
            tracing::error!("http server exited with error: {error}");
        }
    });

    tracing::info!("server started");

    let runtime_handle = runtime.handle().clone();
    *guard = Some(ServerHandle { runtime, runtime_handle, shutdown: shutdown_tx, state });
}

/// Stop the HTTP server and tear down its runtime. Calling this while the
/// server is not running is a no-op.
#[unsafe(no_mangle)]
pub extern "C" fn ge_rust_stop() {
    let handle = {
        let mut guard = match SERVER.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.take()
    };

    let Some(handle) = handle else {
        tracing::warn!("ge_rust_stop called while server is not running");
        return;
    };

    // Signal the server to begin a graceful shutdown. The receiver may already
    // be gone if the server task exited on its own; that's fine.
    let _ = handle.shutdown.send(());

    // Block until all tasks finish and the runtime is fully torn down.
    handle.runtime.shutdown_timeout(Duration::from_secs(30));

    tracing::info!("server stopped");
}

/// Spawn the YouTube stream-notifier workflow on the running tokio runtime.
/// Reads `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, and `DISCORD_WEBHOOK_URL`
/// from the environment, handles OAuth token acquisition (reusing the axum
/// server for the redirect callback), and posts a Discord notification with
/// the live-stream URL. Returns immediately without blocking the calling thread.
#[unsafe(no_mangle)]
pub extern "C" fn ge_stream_notifier_start() {
    let (runtime_handle, state) = {
        let guard = match SERVER.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match guard.as_ref() {
            Some(h) => (h.runtime_handle.clone(), h.state.clone()),
            None => {
                tracing::error!("ge_stream_notifier_start called but server is not running");
                return;
            }
        }
    };

    runtime_handle.spawn(stream_notifier::run(state));
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_stream_notifier_stop() {
    let (runtime_handle, state) = {
        let guard = match SERVER.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match guard.as_ref() {
            Some(h) => (h.runtime_handle.clone(), h.state.clone()),
            None => {
                tracing::error!("ge_stream_notifier_stop called but server is not running");
                return;
            }
        }
    };

    runtime_handle.spawn(stream_notifier::stop(state));
}