use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::obs::frame_capture::{FRAME_BUFFER_CAPACITY, FrameMailbox, MailboxRecv, ProducerCtx};
use crate::run_monitoring::RunMonitor;

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
        let dir = crate::config::temp_dir().join(format!("ge-frames-{}-{nanos}", std::process::id()));
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
        match super::screenshot::encode_bmp_bgra(bytes, width, height) {
            Ok(data) => {
                if let Err(e) = std::fs::write(&path, data) {
                    tracing::warn!("failed to write dumped frame: {e}");
                }
            }
            Err(e) => tracing::warn!("failed to encode dumped frame: {e}"),
        }
    }
}

/// Resolve Flatpak's sandbox `/tmp` to its host location when known.
/// Otherwise the caller logs the original path.
fn flatpak_host_path(dir: &Path) -> Option<String> {
    // `/.flatpak-info` exists only inside a Flatpak sandbox.
    let info = std::fs::read_to_string("/.flatpak-info").ok()?;
    let app_id = crate::config::flatpak_id()?;
    // The [Instance] section names the per-run instance; log it as a fallback hint.
    let instance = info.lines().find_map(|l| l.trim().strip_prefix("instance-id=")).map(str::trim);
    match (crate::config::xdg_runtime_dir(), dir.strip_prefix("/tmp").ok()) {
        (Some(runtime), Some(rel)) => Some(format!("{runtime}/.flatpak/{app_id}/tmp/{}", rel.display())),
        _ => Some(format!("under the host Flatpak runtime dir for {app_id} (instance {})", instance.unwrap_or("?"))),
    }
}

/// A standalone frame dump owns its capture callback, mailbox, and worker.
/// It writes frames to disk independently of whether monitoring is active.
struct FrameDumpHandle {
    mailbox: Arc<FrameMailbox>,
    producer: crate::obs::RegisteredRenderCallback<ProducerCtx>,
    thread: JoinHandle<()>,
    source_name: String,
}

#[derive(Debug)]
pub(crate) enum DumpStartError {
    InvalidSource,
    CaptureUnavailable,
    WorkerUnavailable,
}

pub(crate) struct FrameDumper {
    active: Mutex<Option<FrameDumpHandle>>,
    monitor: Arc<RunMonitor>,
}

impl FrameDumper {
    pub(crate) fn new(monitor: Arc<RunMonitor>) -> Self {
        Self { active: Mutex::new(None), monitor }
    }

    /// Dump source frames, sharing the monitor's crop transform when sources match.
    /// Otherwise capture uses the uncalibrated `WORK_HEIGHT` downscale.
    pub(crate) fn start(&self, source_name: String) -> Result<(), DumpStartError> {
        let name = CString::new(source_name.clone()).map_err(|_| DumpStartError::InvalidSource)?;

        // Double-buffered so readback pipelines without stalling OBS's render thread.
        let Some(ctx) = crate::obs::CaptureContext::new(true) else {
            return Err(DumpStartError::CaptureUnavailable);
        };

        let mailbox = Arc::new(FrameMailbox::new(FRAME_BUFFER_CAPACITY));
        let region = self.monitor.capture_region(&source_name).unwrap_or_else(|| Arc::new(Mutex::new(None)));
        let producer = crate::obs::RegisteredRenderCallback::register(ProducerCtx {
            ctx,
            name,
            region,
            mailbox: mailbox.clone(),
            timing_enabled: false,
            last_callback_at: Mutex::new(None),
        });

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
                drop(producer);
                return Err(DumpStartError::WorkerUnavailable);
            }
        };

        let mut guard = self.active.lock().unwrap_or_else(|p| p.into_inner());
        *guard = Some(FrameDumpHandle { mailbox, producer, thread, source_name });
        tracing::info!("frame dump started");
        Ok(())
    }

    /// Stop the active frame dump, if any. Returns `false` when none was running.
    /// Teardown mirrors [`crate::run_monitoring::RunMonitor::stop`]: unregister the callback (fences further
    /// callbacks), close the mailbox to wake+join the worker, then free the producer.
    pub(crate) async fn stop(&self) -> bool {
        let handle = {
            let mut guard = self.active.lock().unwrap_or_else(|p| p.into_inner());
            guard.take()
        };
        let Some(handle) = handle else {
            return false;
        };

        tokio::task::spawn_blocking(move || {
            let FrameDumpHandle { mailbox, producer, thread, source_name } = handle;
            drop(producer);
            mailbox.close();
            if thread.join().is_err() {
                tracing::error!("frame dump thread panicked");
            }
            tracing::info!(source = %source_name, "frame dump stopped");
        })
        .await
        .ok();

        true
    }
}
