mod config;
pub mod cv;
mod ffi;
mod http;
mod stream_notifier;
mod timer;

use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use http::{AppState, AppStateInner};
use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

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
        let subscriber =
            tracing_subscriber::fmt().with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!(
                    "{}={level},tower_http={level}",
                    env!("CARGO_CRATE_NAME"),
                    level = if cfg!(debug_assertions) { "debug" } else { "info" }
                )
                .into()
            }));

        subscriber.init();
    }

    // Resolve (and log) all configuration once, right after logging is set up.
    let config = Config::from_env();

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
        monitor: std::sync::Mutex::new(None),
        config,
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
/// Accepts OBS service settings as JSON and posts a Discord notification with
/// the live-stream URL. Returns immediately without blocking the calling thread.
#[unsafe(no_mangle)]
pub extern "C" fn ge_stream_notifier_start(service_settings_json: *const c_char) {
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

    let settings_json = if service_settings_json.is_null() {
        tracing::warn!("ge_stream_notifier_start called with null settings JSON pointer");
        "{}".to_string()
    } else {
        // SAFETY: The caller guarantees this points to a valid NUL-terminated C string
        // for the duration of this function call. We copy into an owned String
        // immediately, so no borrowed lifetime escapes this boundary.
        let cstr = unsafe { CStr::from_ptr(service_settings_json) };
        cstr.to_string_lossy().into_owned()
    };

    runtime_handle.spawn(stream_notifier::run(state, settings_json));
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
