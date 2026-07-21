mod browser;
mod browser_dock;
pub mod config;
pub mod cv;
mod db;
mod ffi;
mod ffmpeg;
pub mod ge;
mod http;
mod logging;
pub mod models;
mod recording;
mod settings;
mod stream_notifier;
mod template_tokens;
mod timer;
mod update_apply;
mod updates;
mod youtube;

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use http::{
    AppSnapshot,
    AppState,
    AppStateInner,
    MonitorEvent,
    MonitorSnapshot,
    MonitorStoppedReason,
    RecordingStateStore,
    SharedStateStore,
};
use tokio::runtime::Runtime;
use tokio::sync::oneshot;

use crate::settings::{SettingsReload, SettingsStore};

pub(crate) const PLUGIN_VERSION: &str = env!("GE_PLUGIN_VERSION");

pub(crate) type ObsPathGetter = unsafe extern "C" fn(*mut c_char, usize) -> bool;

pub(crate) fn read_obs_path(getter: ObsPathGetter) -> Option<PathBuf> {
    let mut buffer = vec![0 as c_char; 4096];
    let ok = unsafe { getter(buffer.as_mut_ptr(), buffer.len()) };
    if !ok {
        return None;
    }

    let path = unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy().into_owned();
    if path.is_empty() { None } else { Some(PathBuf::from(path)) }
}

fn existing_template_dir(candidate: impl AsRef<Path>) -> Option<PathBuf> {
    let candidate = candidate.as_ref();
    if !candidate.is_dir() {
        return None;
    }
    Some(candidate.canonicalize().unwrap_or_else(|_| candidate.to_path_buf()))
}

fn resolve_cv_template_dir(data_path: Option<&Path>, binary_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = data_path.and_then(|path| existing_template_dir(path.join("cv_templates"))) {
        return Some(path);
    }

    let binary_dir = binary_path.and_then(Path::parent)?;
    ["../../data/cv_templates", "../Resources/cv_templates", "../cv_templates", "cv_templates"]
        .into_iter()
        .find_map(|relative| existing_template_dir(binary_dir.join(relative)))
}

fn configure_cv_template_dir() {
    let data_path = read_obs_path(ffi::ge_obs_module_data_path);
    let binary_path = read_obs_path(ffi::ge_obs_module_binary_path);

    let Some(template_dir) = resolve_cv_template_dir(data_path.as_deref(), binary_path.as_deref()) else {
        tracing::warn!(
            data_path = ?data_path,
            binary_path = ?binary_path,
            "OBS did not resolve the bundled CV templates directory"
        );
        return;
    };

    tracing::debug!(template_dir = %template_dir.display(), "resolved bundled CV templates directory");
    cv::set_template_dir(template_dir.to_string_lossy().into_owned());
}

/// Ensures the OBS custom browser dock is registered during OBS module post-load.
#[unsafe(no_mangle)]
pub extern "C" fn ge_browser_dock_post_load() {
    browser_dock::post_load();
}

#[cfg(feature = "test-hooks")]
pub fn ge_test_write_tagged_clip(input: &Path, output: &Path, status: &str, timestamp: &str) {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).expect("create tagged clip parent");
    }
    let duration = ffmpeg::duration_secs(input).expect("probe tagged clip input");
    let metadata = ffmpeg::ClipMetadata {
        timestamp: timestamp.to_owned(),
        time: Some("02:03".to_owned()),
        time_seconds: Some(123),
        level: "Surface 2".to_owned(),
        level_number: Some(8),
        difficulty: Some("00 Agent".to_owned()),
        status: status.parse().expect("valid run status"),
        rom_language: "en".to_owned(),
        source_name: "N64 Capture".to_owned(),
        comment: "Created by The Golden Eye OBS plugin test".to_owned(),
        plugin_version: "test".to_owned(),
    };
    ffmpeg::trim_with_metadata(input, output, 1.0, (duration - 1.0).max(2.0), Some(&metadata))
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

/// The shim's canonical on-disk path for *this* core, set via `ge_rust_set_core_path`.
/// NOT the dlopen'd temp copy nor the shim's `ge_obs_module_binary_path`; `update_apply`
/// needs it to stage updates. A `Mutex` (not `OnceLock`) so each load can reset it.
static CORE_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub(crate) fn core_path() -> Option<PathBuf> {
    CORE_PATH.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

/// Called by the C core (`ge_core_load`) with the shim's canonical path for this core.
/// # Safety
/// `path` must be null or a valid NUL-terminated C string for this call; copied at once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ge_rust_set_core_path(path: *const c_char) {
    if path.is_null() {
        return;
    }
    // SAFETY: the caller guarantees a valid NUL-terminated string for the
    // duration of this call; copied into an owned PathBuf immediately.
    let path = unsafe { CStr::from_ptr(path) }.to_string_lossy().into_owned();
    *CORE_PATH.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(PathBuf::from(path));
}

/// Whether *this* core load followed a successful update apply, set by
/// `ge_rust_set_was_reloaded` before `ge_rust_start()`. Read once into
/// `reloaded_at` so a client can be told "the plugin just updated".
static WAS_RELOADED: AtomicBool = AtomicBool::new(false);

/// Called by the C core (`ge_core_load`) to report whether this load followed
/// a reload (an applied update) rather than a cold OBS start or a rollback.
#[unsafe(no_mangle)]
pub extern "C" fn ge_rust_set_was_reloaded(was_reloaded: bool) {
    WAS_RELOADED.store(was_reloaded, Ordering::Release);
}
/// Whether OBS began its replay-buffer stop while a monitor was still active.
/// Intentional shutdown removes the monitor first; an unexpected OBS stop doesn't.
/// Snapshot at STOPPING so a stale STOPPED event can't tear down a replacement monitor.
static REPLAY_STOP_SHOULD_STOP_MONITOR: AtomicBool = AtomicBool::new(false);

// Also included, unconditionally, by the `test_match`/`annotate_match` bin
// crates (see src/bin/*.rs) so their builds can resolve the same symbols.
#[cfg(test)]
#[path = "obs_stub.rs"]
mod obs_stub;

/// Start the HTTP server on a background tokio runtime; returns immediately.
/// A no-op returning `true` if already running. Returns `false` if the runtime
/// or port bind failed -- the caller must treat that as a load failure.
#[unsafe(no_mangle)]
pub extern "C" fn ge_rust_start() -> bool {
    logging::init();

    configure_cv_template_dir();

    let settings = SettingsStore::load_default();
    let catalog_was_missing = !crate::db::run_catalog::RunCatalog::exists_for_settings(settings.path());
    let run_catalog = match crate::db::run_catalog::RunCatalog::open_for_settings(settings.path()) {
        Ok(catalog) => Arc::new(catalog),
        Err(error) => {
            tracing::error!("failed to open run catalog: {error:#}");
            return false;
        }
    };
    let mut guard = match SERVER.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    if guard.is_some() {
        tracing::warn!("ge_rust_start called while server is already running");
        return true;
    }

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
                tracing::error!("failed to bind port {}: {error}", http::SERVER_PORT);
                return false;
            }
        }
    };

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let was_reloaded = WAS_RELOADED.load(Ordering::Acquire);

    let snapshot = SharedStateStore::new(AppSnapshot {
        monitor: MonitorSnapshot { enabled: false, source_name: None },
        level_match: None,
        recording_state: None,
        sources: Vec::new(),
        replay_buffer: http::ReplayBufferStatus::unknown(),
        settings_status: settings.status_without_runtime_defaults(),
        update: None,
    });
    // One-off monitor events (recording saved, ...). Capacity bounds how far a
    // slow client can lag before it drops events; the worker ignores send errors,
    // so a full/empty channel never blocks frame processing.
    let (event_tx, _) = tokio::sync::broadcast::channel(64);
    let (frontend_ready_tx, _) = tokio::sync::watch::channel(was_reloaded);
    let recording_state = RecordingStateStore::new(snapshot.clone());
    let state = Arc::new(AppStateInner {
        oauth_pending: tokio::sync::Mutex::new(None),
        youtube: youtube::YoutubeUploadStore::new(settings.path(), run_catalog.clone()),
        stream_message: tokio::sync::Mutex::new(None),
        monitor: std::sync::Mutex::new(None),
        snapshot,
        event_tx,
        recording_state,
        monitor_annotations_enabled: AtomicBool::new(false),
        frame_dump: std::sync::Mutex::new(None),
        frontend_ready_tx,
        run_catalog,
        run_catalog_needs_seed: Mutex::new(catalog_was_missing),
        settings,
        reloaded_at: was_reloaded.then(std::time::Instant::now),
    });

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
    runtime.spawn(watch_settings_file(state.clone()));
    runtime.spawn(updates::check_for_updates_on_startup(state.clone()));
    runtime.spawn(update_apply::auto_apply_when_safe(state.clone()));

    tracing::info!("server started");

    let runtime_handle = runtime.handle().clone();
    *guard = Some(ServerHandle { runtime, runtime_handle, shutdown: shutdown_tx, state });
    true
}

async fn watch_settings_file(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        match state.settings.reload_from_disk_if_changed() {
            SettingsReload::Unchanged => {}
            SettingsReload::Reloaded(settings) => {
                state.snapshot.set_settings_status(state.settings.status_without_runtime_defaults());
                let _ = state.event_tx.send(MonitorEvent::SettingsReloaded {
                    config_path: state.settings.path().to_string_lossy().into_owned(),
                    settings: *settings,
                });
            }
            SettingsReload::Invalid(error) => {
                state.snapshot.set_settings_status(state.settings.status_without_runtime_defaults());
                let _ = state.event_tx.send(MonitorEvent::SettingsInvalid {
                    config_path: state.settings.path().to_string_lossy().into_owned(),
                    error,
                });
            }
        }
    }
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

    // Dev hot reload deliberately permits an active monitor/recording. Stop and
    // join it before the shim unloads this core, so no old Rust code runs after
    // its library is closed. Production updates are gated before reaching here.
    if cfg!(feature = "dev") {
        let state = handle.state.clone();
        let _ = handle.runtime_handle.block_on(http::stop_monitor(&state));
    }

    // Signal the server to begin a graceful shutdown. The receiver may already
    // be gone if the server task exited on its own; that's fine.
    let _ = handle.shutdown.send(());

    // Block until all tasks finish and the runtime is fully torn down.
    handle.runtime.shutdown_timeout(Duration::from_secs(30));

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

    runtime_handle.spawn(stream_notifier::run(state, settings_json));
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
    state.snapshot.set_sources(http::collect_sources());
    refresh_runtime_snapshot(&state);
}

fn frontend_ready(state: &AppState) -> bool {
    *state.frontend_ready_tx.borrow()
}

fn refresh_runtime_snapshot(state: &AppState) {
    if frontend_ready(state) {
        state.snapshot.set_settings_status(state.settings.status());
        state.snapshot.set_replay_buffer(http::current_replay_buffer_status());
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

    state.snapshot.set_sources(http::collect_sources());
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
        state.snapshot.set_replay_buffer(http::current_replay_buffer_status());
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
    recording::on_replay_saved(path);
}

/// Called from the OBS frontend event callback on
/// `OBS_FRONTEND_EVENT_REPLAY_BUFFER_STARTING`.
#[unsafe(no_mangle)]
pub extern "C" fn ge_replay_buffer_starting() {
    recording::on_replay_buffer_starting();
    refresh_replay_buffer_snapshot();
}

/// Called from the OBS frontend event callback on
/// `OBS_FRONTEND_EVENT_REPLAY_BUFFER_STARTED`.
#[unsafe(no_mangle)]
pub extern "C" fn ge_replay_buffer_started() {
    recording::on_replay_buffer_started();
    refresh_replay_buffer_snapshot();
}

/// Called from the OBS frontend event callback on
/// `OBS_FRONTEND_EVENT_REPLAY_BUFFER_STOPPING`.
#[unsafe(no_mangle)]
pub extern "C" fn ge_replay_buffer_stopping() {
    let monitor_active = {
        let guard = SERVER.lock().unwrap_or_else(|p| p.into_inner());
        guard.as_ref().is_some_and(|handle| handle.state.monitor.lock().unwrap_or_else(|p| p.into_inner()).is_some())
    };
    REPLAY_STOP_SHOULD_STOP_MONITOR.store(monitor_active, Ordering::Release);
    recording::on_replay_buffer_stopping();
    refresh_replay_buffer_snapshot();
}

/// Called from the OBS frontend event callback on
/// `OBS_FRONTEND_EVENT_REPLAY_BUFFER_STOPPED`.
#[unsafe(no_mangle)]
pub extern "C" fn ge_replay_buffer_stopped() {
    recording::on_replay_buffer_stopped();
    refresh_replay_buffer_snapshot();

    if !REPLAY_STOP_SHOULD_STOP_MONITOR.swap(false, Ordering::AcqRel) {
        return;
    }

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

    runtime_handle.spawn(async move {
        if http::stop_monitor(&state).await {
            tracing::warn!("replay buffer stopped while monitoring was active; monitoring disabled");
            let _ =
                state.event_tx.send(MonitorEvent::MonitorStopped { reason: MonitorStoppedReason::ReplayBufferStopped });
        }
    });
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

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
