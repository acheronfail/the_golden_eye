mod api_contract;
mod app;
pub mod config;
mod desktop;
mod diagnostics;
mod http;
mod logging;
mod obs;
mod plugin_updates;
mod run_library;
mod run_monitoring;
mod settings;
mod streaming_notifications;
mod template_tokens;
mod youtube_uploads;

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(feature = "test-hooks")]
use ge_clip::ClipMetadata;
#[cfg(feature = "test-hooks")]
pub use obs::GeCaptureRegion;
use tokio::runtime::Runtime;
use tokio::sync::oneshot;

use crate::app::AppState;
use crate::run_monitoring::StopReason;
use crate::run_monitoring::replay_buffer::{REPLAY_BUFFER, ReplayEvent};
use crate::settings::SettingsStore;

pub(crate) const PLUGIN_VERSION: &str = env!("GE_PLUGIN_VERSION");
pub(crate) const UPDATER_VERSION: &str = env!("GE_UPDATER_VERSION");

pub use api_contract::export_api_contract;

fn existing_template_dir(candidate: impl AsRef<Path>) -> Option<PathBuf> {
    let candidate = candidate.as_ref();
    if !candidate.is_dir() {
        return None;
    }
    Some(candidate.canonicalize().unwrap_or_else(|_| candidate.to_path_buf()))
}

fn resolve_cv_template_dir(data_path: Option<&Path>) -> Option<PathBuf> {
    data_path.and_then(|path| existing_template_dir(path.join("cv_templates")))
}

fn configure_cv_template_dir() {
    let data_path = obs::module_data_path();

    let Some(template_dir) = resolve_cv_template_dir(data_path.as_deref()) else {
        tracing::warn!(data_path = ?data_path, "OBS did not resolve the bundled CV templates directory");
        return;
    };

    tracing::debug!(template_dir = %template_dir.display(), "resolved bundled CV templates directory");
    ge_cv::set_template_dir(template_dir.to_string_lossy().into_owned());
}

fn configure_cv_runtime() {
    ge_cv::configure(ge_cv::RuntimeConfig {
        debug: config::cv_debug_enabled(),
        timing: config::cv_timing_enabled(),
        threads_overridden: config::cv_threads_overridden(),
    });
}

/// Ensures the OBS custom browser dock is registered during OBS module post-load.
#[unsafe(no_mangle)]
pub extern "C" fn ge_browser_dock_post_load() {
    obs::browser_dock::post_load();
}

#[cfg(feature = "test-hooks")]
pub fn ge_test_write_tagged_clip(input: &Path, output: &Path, status: &str, timestamp: &str) {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).expect("create tagged clip parent");
    }
    let duration = ge_media::duration_secs(input).expect("probe tagged clip input");
    let metadata = ClipMetadata {
        run_id: String::new(),
        timestamp: timestamp.to_owned(),
        time: Some("02:03".to_owned()),
        time_seconds: Some(123),
        level: "Surface 2".to_owned(),
        level_number: Some(8),
        difficulty: Some("00 Agent".to_owned()),
        status: status.parse().expect("valid run status"),
        was_personal_best: false,
        game_language: "en".to_owned(),
        rom_version: None,
        source_name: "N64 Capture".to_owned(),
        comment: "Created by The Golden Eye OBS plugin test".to_owned(),
        plugin_version: "test".to_owned(),
        retention_state: "kept".to_owned(),
        retention_reason: Some("imported".to_owned()),
    };
    ge_media::trim_with_metadata(input, output, 1.0, (duration - 1.0).max(2.0), Some(&metadata))
        .expect("write tagged clip");
}

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
static PENDING_RUNTIME_DATA: Mutex<Option<plugin_updates::installation::RuntimeDataTransaction>> = Mutex::new(None);

#[derive(Clone)]
struct UpdatePaths {
    core: PathBuf,
    staged_dir: PathBuf,
}

/// Durable paths resolved by the resident loader. A Mutex lets every core load
/// replace them, including a rollback after a failed update.
static UPDATE_PATHS: Mutex<Option<UpdatePaths>> = Mutex::new(None);

pub(crate) fn core_path() -> Option<PathBuf> {
    UPDATE_PATHS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref().map(|paths| paths.core.clone())
}

pub(crate) fn staged_update_dir() -> Option<PathBuf> {
    UPDATE_PATHS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref().map(|paths| paths.staged_dir.clone())
}

/// Called by the C core with paths resolved by the resident loader.
/// # Safety
/// Both pointers must reference valid NUL-terminated strings for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ge_runtime_set_update_paths(core_path: *const c_char, staged_dir: *const c_char) {
    if core_path.is_null() || staged_dir.is_null() {
        return;
    }
    // SAFETY: the caller keeps both strings valid for this call.
    let core = unsafe { CStr::from_ptr(core_path) }.to_string_lossy().into_owned();
    let staged_dir = unsafe { CStr::from_ptr(staged_dir) }.to_string_lossy().into_owned();
    *UPDATE_PATHS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) =
        Some(UpdatePaths { core: PathBuf::from(core), staged_dir: PathBuf::from(staged_dir) });
}

static FRONTEND_READY_ON_LOAD: AtomicBool = AtomicBool::new(false);
static APPLYING_UPDATE: AtomicBool = AtomicBool::new(false);

/// The C core separates frontend readiness from provisional update startup.
#[unsafe(no_mangle)]
pub extern "C" fn ge_runtime_set_load_context(frontend_ready: bool, applying_update: bool) {
    FRONTEND_READY_ON_LOAD.store(frontend_ready, Ordering::Release);
    APPLYING_UPDATE.store(applying_update, Ordering::Release);
}
// Standalone test executables never call OBS, but still need these symbols.
#[cfg(test)]
#[path = "obs_stub.rs"]
mod obs_stub;

/// Start the HTTP server on a background tokio runtime; returns immediately.
/// A no-op returning `true` if already running. Returns `false` if the runtime
/// or port bind failed -- the caller must treat that as a load failure.
#[unsafe(no_mangle)]
pub extern "C" fn ge_runtime_start() -> bool {
    logging::init();

    let mut guard = match SERVER.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if guard.is_some() {
        tracing::warn!("ge_runtime_start called while server is already running");
        return true;
    }

    let applying_update = APPLYING_UPDATE.load(Ordering::Acquire);
    let frontend_ready = FRONTEND_READY_ON_LOAD.load(Ordering::Acquire);
    let data_transaction = if applying_update {
        match plugin_updates::installation::install_staged_runtime_data() {
            Ok(transaction) => Some(transaction),
            Err(error) => {
                tracing::error!("failed to install staged runtime data: {error:#}");
                return false;
            }
        }
    } else {
        None
    };

    configure_cv_runtime();
    configure_cv_template_dir();

    let settings = Arc::new(SettingsStore::load_default());
    let catalog_was_missing = !ge_catalog::run_catalog::RunCatalog::exists_for_settings(settings.path());
    let run_catalog = match ge_catalog::run_catalog::RunCatalog::open_for_settings(settings.path()) {
        Ok(catalog) => Arc::new(catalog),
        Err(error) => {
            tracing::error!("failed to open run catalog: {error:#}");
            return false;
        }
    };
    let catalog_needs_seed = catalog_was_missing || run_catalog.needs_seed();

    let runtime = match Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!("failed to create tokio runtime: {error}");
            return false;
        }
    };

    // Bind synchronously in the runtime's context so a bind failure (e.g. port
    // still held by a previous instance) is reported to the caller now, not later
    // inside a spawned task ge_core_load can't see.
    let listener = {
        let _guard = runtime.enter();
        match http::bind_listener() {
            Ok(listener) => listener,
            Err(error) => {
                tracing::error!("failed to bind port {}: {error}", config::server_port());
                return false;
            }
        }
    };

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let state = app::build_state(settings, run_catalog, catalog_needs_seed, frontend_ready, applying_update);

    if let Some(transaction) = data_transaction {
        let mut pending = PENDING_RUNTIME_DATA.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if pending.is_some() {
            tracing::error!("a runtime data transaction is already pending");
            return false;
        }
        *pending = Some(transaction);
    }

    // Spawn the server onto the runtime. `spawn` returns immediately so the
    // C caller is never blocked; the runtime drives the future on its own
    // worker threads.
    let state_clone = state.clone();
    tracing::info!(version = PLUGIN_VERSION, "starting server");
    runtime.spawn(async move {
        if let Err(error) = http::serve(listener, shutdown_rx, state_clone).await {
            tracing::error!("http server exited with error: {error}");
        }
    });
    runtime.spawn(app::watch_settings_file(state.clone()));
    runtime.spawn(state.updates.clone().check_for_updates_on_startup());
    runtime.spawn(state.updates.clone().auto_apply_when_safe());

    tracing::info!("server started");

    let runtime_handle = runtime.handle().clone();
    *guard = Some(ServerHandle { runtime, runtime_handle, shutdown: shutdown_tx, state });
    true
}

/// Commits runtime data after the loader has durably replaced the canonical core.
#[unsafe(no_mangle)]
pub extern "C" fn ge_runtime_commit_update() {
    let transaction = PENDING_RUNTIME_DATA.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
    if let Some(transaction) = transaction {
        transaction.commit();
        let state = SERVER.lock().unwrap_or_else(|p| p.into_inner()).as_ref().map(|server| server.state.clone());
        if let Some(state) = state {
            let mut committed = state.update_committed_at.lock().unwrap_or_else(|p| p.into_inner());
            if committed.is_none() {
                state.updates.finish_apply();
                *committed = Some(std::time::Instant::now());
                let _ = state.event_tx.send(state.update_applied_event());
            }
        }
    } else {
        tracing::warn!("ge_runtime_commit_update called without a pending runtime data transaction");
    }
}

/// Stop the HTTP server and tear down its runtime. Calling this while the
/// server is not running is a no-op.
#[unsafe(no_mangle)]
pub extern "C" fn ge_runtime_stop() {
    let handle = {
        let mut guard = match SERVER.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.take()
    };

    let Some(handle) = handle else {
        tracing::warn!("ge_runtime_stop called while server is not running");
        return;
    };

    // Join any active monitor before the loader unloads this core, and persist a
    // truthful session end reason for both development reloads and OBS shutdown.
    let state = handle.state.clone();
    let end_reason = if cfg!(feature = "dev") { StopReason::CoreReload } else { StopReason::ObsShutdown };
    handle.runtime_handle.block_on(state.frame_dump.stop());
    let _ = handle.runtime_handle.block_on(state.monitor.stop(end_reason));

    // Signal the server to begin a graceful shutdown. The receiver may already
    // be gone if the server task exited on its own; that's fine.
    let _ = handle.shutdown.send(());

    // Block until all tasks finish and the runtime is fully torn down.
    handle.runtime.shutdown_timeout(Duration::from_secs(30));

    // A normal unload after a committed update has nothing pending. Closing a
    // newly loaded core before commit drops this transaction and restores data.
    drop(PENDING_RUNTIME_DATA.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take());

    tracing::info!("server stopped");
}

/// Spawn the YouTube stream-notifier on the tokio runtime; posts a Discord notification
/// with the live-stream URL from OBS service-settings JSON. Returns immediately.
/// # Safety
/// `service_settings_json` must be null or a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ge_stream_notifier_start(service_settings_json: *const c_char) {
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

    runtime_handle.spawn(async move { state.notifications.start(settings_json).await });
}

/// Called from the C core when OBS emits `OBS_FRONTEND_EVENT_FINISHED_LOADING`.
/// This is the first lifecycle point where replay-buffer frontend APIs are safe
/// to query on all supported OBS startup paths observed so far.
#[unsafe(no_mangle)]
pub extern "C" fn ge_frontend_finished_loading() {
    let state = {
        let guard = match SERVER.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.as_ref().map(|h| h.state.clone())
    };

    let Some(state) = state else {
        tracing::warn!("ge_frontend_finished_loading called but server is not running");
        return;
    };

    state.frontend_ready_tx.send_replace(true);
    state.snapshot.set_sources(obs::collect_sources());
    refresh_runtime_snapshot(&state);
}

fn frontend_ready(state: &AppState) -> bool {
    *state.frontend_ready_tx.borrow()
}

fn refresh_runtime_snapshot(state: &AppState) {
    if frontend_ready(state) {
        state.snapshot.set_settings_status(state.settings.status());
        state.snapshot.set_replay_buffer(obs::current_replay_buffer_status());
    } else {
        state.snapshot.set_settings_status(state.settings.status_without_runtime_defaults());
    }
}

/// Called from the C core when OBS reports that the source graph changed.
/// Recollects the current renderable video sources and pushes the snapshot to
/// connected browser clients.
#[unsafe(no_mangle)]
pub extern "C" fn ge_sources_changed() {
    let state = {
        let guard = match SERVER.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match guard.as_ref() {
            Some(h) => h.state.clone(),
            None => {
                tracing::warn!("ge_sources_changed called but server is not running");
                return;
            }
        }
    };

    if !frontend_ready(&state) {
        tracing::debug!("skipping source refresh until OBS frontend is ready");
        return;
    }

    state.snapshot.set_sources(obs::collect_sources());
}

fn refresh_replay_buffer_snapshot() {
    let state = {
        let guard = match SERVER.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.as_ref().map(|h| h.state.clone())
    };
    if let Some(state) = state
        && frontend_ready(&state)
    {
        state.snapshot.set_replay_buffer(obs::current_replay_buffer_status());
    }
}

/// Called on `OBS_FRONTEND_EVENT_REPLAY_BUFFER_SAVED` with the saved replay path
/// (may be null/empty). Wakes the blocked recording save so we never poll.
/// # Safety
/// `path` must be null or a valid NUL-terminated C string for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ge_replay_buffer_saved(path: *const c_char) {
    let path = if path.is_null() {
        None
    } else {
        // SAFETY: OBS passes a valid NUL-terminated C string; we copy it into an
        // owned String immediately, so no borrowed lifetime escapes.
        let s = unsafe { CStr::from_ptr(path) }.to_string_lossy().into_owned();
        if s.is_empty() { None } else { Some(s) }
    };
    REPLAY_BUFFER.handle(ReplayEvent::Saved(path));
}

/// Called from the OBS frontend event callback on
/// `OBS_FRONTEND_EVENT_REPLAY_BUFFER_STARTING`.
#[unsafe(no_mangle)]
pub extern "C" fn ge_replay_buffer_starting() {
    REPLAY_BUFFER.handle(ReplayEvent::Starting);
    refresh_replay_buffer_snapshot();
}

/// Called from the OBS frontend event callback on
/// `OBS_FRONTEND_EVENT_REPLAY_BUFFER_STARTED`.
#[unsafe(no_mangle)]
pub extern "C" fn ge_replay_buffer_started() {
    REPLAY_BUFFER.handle(ReplayEvent::Started);
    refresh_replay_buffer_snapshot();
}

/// Called from the OBS frontend event callback on
/// `OBS_FRONTEND_EVENT_REPLAY_BUFFER_STOPPING`.
#[unsafe(no_mangle)]
pub extern "C" fn ge_replay_buffer_stopping() {
    {
        let guard = SERVER.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(handle) = guard.as_ref() {
            handle.state.monitor.replay_buffer_stopping();
        }
    }
    REPLAY_BUFFER.handle(ReplayEvent::Stopping);
    refresh_replay_buffer_snapshot();
}

/// Called from the OBS frontend event callback on
/// `OBS_FRONTEND_EVENT_REPLAY_BUFFER_STOPPED`.
#[unsafe(no_mangle)]
pub extern "C" fn ge_replay_buffer_stopped() {
    REPLAY_BUFFER.handle(ReplayEvent::Stopped);
    refresh_replay_buffer_snapshot();

    let (runtime_handle, state) = {
        let guard = match SERVER.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match guard.as_ref() {
            Some(h) => (h.runtime_handle.clone(), h.state.clone()),
            None => {
                tracing::warn!("ge_replay_buffer_stopped called but server is not running");
                return;
            }
        }
    };

    if state.monitor.take_replay_stop_request() {
        runtime_handle.spawn(async move {
            state.monitor.stop(StopReason::ReplayBufferStopped).await;
        });
    }
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

    runtime_handle.spawn(async move { state.notifications.stop().await });
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
