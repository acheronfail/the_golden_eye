use std::env;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Result;

use super::capture::{FRAME_BUFFER_CAPACITY, FrameMailbox, MailboxRecv, ProducerCtx, ProducerPtr, ge_frame_callback};
use crate::http::AppState;

/// Developer diagnostic: dumps each captured (matcher-input) frame to a temp
/// directory as BMP so a live capture-card feed can be compared pixel-for-pixel
/// against the same content played from a file.
struct FrameDump {
    dir: PathBuf,
    index: u64,
}

impl FrameDump {
    fn new() -> std::io::Result<Self> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let dir = env::temp_dir().join(format!("ge-frames-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        // Inside the OBS Flatpak the sandbox path isn't where the user finds the
        // files, so surface a best-effort host path alongside it when we can.
        match flatpak_host_path(&dir) {
            Some(host) => {
                tracing::info!(sandbox_path = %dir.display(), host_path = %host, "dumping frames to disk (Flatpak)");
            }
            None => tracing::info!(dir = %dir.display(), "dumping frames to disk"),
        }
        Ok(Self { dir, index: 0 })
    }

    fn write(&mut self, bytes: &[u8], width: u32, height: u32) {
        let path = self.dir.join(format!("frame-{:06}.bmp", self.index));
        self.index += 1;
        match crate::http::routes::screenshot::encode_bmp_bgra(bytes, width, height) {
            Ok(data) => {
                if let Err(e) = std::fs::write(&path, data) {
                    tracing::warn!("failed to write dumped frame: {e}");
                }
            }
            Err(e) => tracing::warn!("failed to encode dumped frame: {e}"),
        }
    }
}

/// Best-effort host path for a dump dir when running inside a Flatpak sandbox
/// (e.g. the OBS Studio Flatpak on Linux). The sandbox remaps the filesystem, so
/// the in-sandbox path the process sees isn't where the user finds the files;
/// the sandbox `/tmp` is bind-mounted from `$XDG_RUNTIME_DIR/.flatpak/<app>/tmp`
/// on the host. Returns None when not in a Flatpak or the mapping is unknown, so
/// the caller just logs the raw path.
fn flatpak_host_path(dir: &Path) -> Option<String> {
    // `/.flatpak-info` exists only inside a Flatpak sandbox.
    let info = std::fs::read_to_string("/.flatpak-info").ok()?;
    let app_id = env::var("FLATPAK_ID").ok()?;
    // The [Instance] section names the per-run instance; log it as a fallback hint.
    let instance = info.lines().find_map(|l| l.trim().strip_prefix("instance-id=")).map(str::trim);
    match (env::var("XDG_RUNTIME_DIR").ok(), dir.strip_prefix("/tmp").ok()) {
        (Some(runtime), Some(rel)) => Some(format!("{runtime}/.flatpak/{app_id}/tmp/{}", rel.display())),
        _ => Some(format!("under the host Flatpak runtime dir for {app_id} (instance {})", instance.unwrap_or("?"))),
    }
}

/// A running standalone frame dump. Mirrors [`MonitorHandle`]'s capture ownership
/// (render callback + capture context + mailbox + worker thread), but its worker
/// writes each frame to disk instead of matching. Independent of the monitor: it
/// runs whenever the developer switch is on, whether or not a monitor is active.
pub struct FrameDumpHandle {
    mailbox: Arc<FrameMailbox>,
    producer: ProducerPtr,
    thread: JoinHandle<()>,
    source_name: String,
}

#[derive(serde::Deserialize)]
pub struct FrameDumpParams {
    enabled: bool,
    /// Source to dump; required when enabling. Ignored when disabling.
    #[serde(default)]
    source: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameDumpResponse {
    frame_dump_enabled: bool,
}

/// Toggles the transient developer frame dump. Any existing dump is stopped
/// first, so this also handles switching to a different source.
#[axum::debug_handler]
pub async fn handle_frame_dump(
    State(state): State<AppState>,
    Json(params): Json<FrameDumpParams>,
) -> Result<Json<FrameDumpResponse>> {
    stop_frame_dump(&state).await;
    if params.enabled {
        let source = params.source.ok_or((StatusCode::BAD_REQUEST, "source is required to enable the frame dump"))?;
        start_frame_dump(&state, source)?;
    }
    Ok(Json(FrameDumpResponse { frame_dump_enabled: params.enabled }))
}

/// Small (status, message) error so the sync starter avoids the large boxed
/// `axum` error type; `?` still lifts it into a handler's response.
type StartResult = std::result::Result<(), (StatusCode, &'static str)>;

/// Start dumping `source_name`'s frames to disk. When a monitor is already
/// running the same source, its latched capture region is shared so dumped
/// frames are cropped/un-stretched identically to what the matcher sees;
/// otherwise the plain `WORK_HEIGHT` downscale is captured (uncalibrated).
pub(crate) fn start_frame_dump(state: &AppState, source_name: String) -> StartResult {
    let name =
        CString::new(source_name.clone()).map_err(|_| (StatusCode::BAD_REQUEST, "source name contains a null byte"))?;

    // Double-buffered so readback pipelines without stalling OBS's render thread.
    let ctx = unsafe { crate::ffi::ge_capture_create(true) };
    if ctx.is_null() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "failed to create capture context"));
    }

    let mailbox = Arc::new(FrameMailbox::new(FRAME_BUFFER_CAPACITY));
    let region = {
        let monitor = state.monitor.lock().unwrap_or_else(|p| p.into_inner());
        match monitor.as_ref() {
            Some(m) if m.source_name == source_name => m.region.clone(),
            _ => Arc::new(Mutex::new(None)),
        }
    };
    let producer =
        Box::into_raw(Box::new(ProducerCtx { ctx, name, region, mailbox: mailbox.clone(), timing_enabled: false }));
    unsafe { crate::ffi::ge_obs_register_frame_callback(ge_frame_callback, producer.cast()) };

    // Write frames on a dedicated OS thread so disk I/O never runs on the OBS
    // graphics thread (the callback) and never ties up the async runtime.
    let worker_mailbox = mailbox.clone();
    let thread = std::thread::Builder::new().name("ge-frame-dump".to_owned()).spawn(move || {
        let mut dump = match FrameDump::new() {
            Ok(dump) => dump,
            Err(err) => {
                tracing::error!("failed to create frame dump directory: {err}");
                return;
            }
        };
        loop {
            match worker_mailbox.recv_until(None) {
                MailboxRecv::Frame(frame) => dump.write(frame.buf.as_slice(), frame.width, frame.height),
                MailboxRecv::Timeout => {}
                MailboxRecv::Closed => break,
            }
        }
        tracing::info!("frame dump loop exiting");
    });
    let thread = match thread {
        Ok(thread) => thread,
        Err(err) => {
            tracing::error!("failed to spawn frame dump thread: {err}");
            unsafe { crate::ffi::ge_obs_unregister_frame_callback(ge_frame_callback, producer.cast()) };
            drop(unsafe { Box::from_raw(producer) });
            return Err((StatusCode::INTERNAL_SERVER_ERROR, "failed to spawn frame dump thread"));
        }
    };

    let mut guard = state.frame_dump.lock().unwrap_or_else(|p| p.into_inner());
    *guard = Some(FrameDumpHandle { mailbox, producer: ProducerPtr(producer), thread, source_name });
    tracing::info!("frame dump started");
    Ok(())
}

/// Stop the active frame dump, if any. Returns `false` when none was running.
/// Teardown mirrors [`stop_monitor`]: unregister the callback (fences further
/// callbacks), close the mailbox to wake+join the worker, then free the producer.
pub(crate) async fn stop_frame_dump(state: &AppState) -> bool {
    let handle = {
        let mut guard = state.frame_dump.lock().unwrap_or_else(|p| p.into_inner());
        guard.take()
    };
    let Some(handle) = handle else {
        return false;
    };

    tokio::task::spawn_blocking(move || {
        let FrameDumpHandle { mailbox, producer, thread, source_name } = handle;
        let producer = producer.0;
        unsafe { crate::ffi::ge_obs_unregister_frame_callback(ge_frame_callback, producer.cast()) };
        mailbox.close();
        if thread.join().is_err() {
            tracing::error!("frame dump thread panicked");
        }
        drop(unsafe { Box::from_raw(producer) });
        tracing::info!(source = %source_name, "frame dump stopped");
    })
    .await
    .ok();

    true
}
