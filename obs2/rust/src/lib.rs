mod browser_dock;
pub mod cv;
mod ffi;
mod ffmpeg;
pub mod ge;
mod http;
mod recording;
mod settings;
mod stream_notifier;
mod timer;
mod updates;

use std::ffi::CStr;
use std::fmt;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;

use http::{AppState, AppStateInner, MonitorEvent, MonitorStoppedReason, RecordingStateStore};
use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tracing::{Event, Subscriber};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::registry::LookupSpan;

use crate::settings::{SettingsReload, SettingsStore};

pub(crate) const PLUGIN_VERSION: &str = env!("GE_PLUGIN_VERSION");

type ObsPathGetter = unsafe extern "C" fn(*mut c_char, usize) -> bool;

fn read_obs_path(getter: ObsPathGetter) -> Option<PathBuf> {
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

/// Ensures the OBS custom browser dock is registered after OBS has completed
/// module loading.
#[unsafe(no_mangle)]
pub extern "C" fn ge_browser_dock_post_load() {
    browser_dock::post_load();
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
static LOGGING_INIT: Once = Once::new();
/// Whether OBS began its current replay-buffer stop while a monitor was still
/// active. Intentional monitor shutdown removes the monitor before requesting
/// the stop; an unexpected OBS stop does not. Snapshotting at STOPPING avoids a
/// stale STOPPED event racing with and tearing down a replacement monitor.
static REPLAY_STOP_SHOULD_STOP_MONITOR: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_replay_buffer_output_directory(_buffer: *mut c_char, _buffer_size: usize) -> bool {
    false
}

struct TheGoldenEyeLogFormat;

impl<S, N> FormatEvent<S, N> for TheGoldenEyeLogFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(&self, ctx: &FmtContext<'_, S, N>, mut writer: Writer<'_>, event: &Event<'_>) -> fmt::Result {
        write!(writer, "[the_golden_eye] ")?;
        tracing_subscriber::fmt::format().without_time().format_event(ctx, writer, event)
    }
}

fn init_logging() {
    LOGGING_INIT.call_once(|| {
        let subscriber = tracing_subscriber::fmt().event_format(TheGoldenEyeLogFormat).with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!(
                    "{}={level},tower_http={level}",
                    env!("CARGO_CRATE_NAME"),
                    level = if cfg!(debug_assertions) { "debug" } else { "info" }
                )
                .into()
            }),
        );

        let _ = subscriber.try_init();
    });
}

/// Start the HTTP server on a background tokio runtime. Returns immediately
/// without blocking the calling (C) thread. Calling this while the server is
/// already running is a no-op.
#[unsafe(no_mangle)]
pub extern "C" fn ge_rust_start() {
    init_logging();

    configure_cv_template_dir();

    let settings = SettingsStore::load_default();

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

    let (match_tx, _) = tokio::sync::watch::channel(None);
    let (recording_state_tx, _) = tokio::sync::watch::channel(None);
    let (source_tx, _) = tokio::sync::watch::channel(http::collect_sources());
    // One-off monitor events (recording saved, ...). Capacity bounds how far a
    // slow client can lag before it drops events; the worker ignores send errors,
    // so a full/empty channel never blocks frame processing.
    let (event_tx, _) = tokio::sync::broadcast::channel(64);
    let recording_state = RecordingStateStore::new(recording_state_tx);
    let state = Arc::new(AppStateInner {
        oauth_pending: tokio::sync::Mutex::new(None),
        stream_message: tokio::sync::Mutex::new(None),
        monitor: std::sync::Mutex::new(None),
        match_tx,
        event_tx,
        recording_state,
        monitor_annotations_enabled: AtomicBool::new(false),
        source_tx,
        update_tx: tokio::sync::watch::channel(None).0,
        settings,
    });

    // Spawn the server onto the runtime. `spawn` returns immediately so the
    // C caller is never blocked; the runtime drives the future on its own
    // worker threads.
    let state_clone = state.clone();
    tracing::info!(version = PLUGIN_VERSION, "starting server");
    runtime.spawn(async move {
        if let Err(error) = http::create_server(shutdown_rx, state_clone).await {
            tracing::error!("http server exited with error: {error}");
        }
    });
    runtime.spawn(watch_settings_file(state.clone()));
    runtime.spawn(updates::check_for_updates_on_startup(state.clone()));

    tracing::info!("server started");

    let runtime_handle = runtime.handle().clone();
    *guard = Some(ServerHandle { runtime, runtime_handle, shutdown: shutdown_tx, state });
}

async fn watch_settings_file(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        match state.settings.reload_from_disk_if_changed() {
            SettingsReload::Unchanged => {}
            SettingsReload::Reloaded(settings) => {
                let _ = state.event_tx.send(MonitorEvent::SettingsReloaded {
                    config_path: state.settings.path().to_string_lossy().into_owned(),
                    settings,
                });
            }
            SettingsReload::Invalid(error) => {
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
///
/// # Safety
/// `service_settings_json` must be null or a valid NUL-terminated C string that
/// stays valid for the duration of this call.
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

    state.source_tx.send_replace(http::collect_sources());
}

/// Called from the OBS frontend event callback on
/// `OBS_FRONTEND_EVENT_REPLAY_BUFFER_SAVED` with the path of the just-saved
/// replay file (may be null/empty). Wakes whichever recording save is blocked
/// waiting for the buffer to finish writing, so we never have to poll.
///
/// # Safety
/// `path` must be null or a valid NUL-terminated C string that stays valid for
/// the duration of this call.
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
}

/// Called from the OBS frontend event callback on
/// `OBS_FRONTEND_EVENT_REPLAY_BUFFER_STARTED`.
#[unsafe(no_mangle)]
pub extern "C" fn ge_replay_buffer_started() {
    recording::on_replay_buffer_started();
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
}

/// Called from the OBS frontend event callback on
/// `OBS_FRONTEND_EVENT_REPLAY_BUFFER_STOPPED`.
#[unsafe(no_mangle)]
pub extern "C" fn ge_replay_buffer_stopped() {
    recording::on_replay_buffer_stopped();

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
mod ffmpeg_link_tests {
    /// Smoke test that the statically-linked FFmpeg is actually callable from
    /// Rust (i.e. the libav* symbols resolve at link time). `version()` just
    /// reads a compiled-in constant, so this purely exercises the linkage.
    #[test]
    fn ffmpeg_links_and_initializes() {
        ffmpeg_next::init().expect("ffmpeg init");
        let v = ffmpeg_next::format::version();
        assert!(v > 0, "libavformat version should be non-zero");
    }
}
