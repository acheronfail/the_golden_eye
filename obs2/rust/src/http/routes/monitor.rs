use std::collections::VecDeque;
use std::env;
use std::ffi::{CString, c_void};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response, Result};
use serde::Deserialize;
use tokio::sync::{broadcast, watch};

use crate::cv::{CaptureRegion, CvMatcher, LevelMatch, Screen};
use crate::ge;
use crate::http::{AppState, MonitorEvent, MonitorFps, MonitorStoppedReason};

const DEFAULT_MONITOR_LANGUAGE: &str = "jp";
const MONITOR_FPS_EMIT_INTERVAL: Duration = Duration::from_millis(100);
/// Frames voted over to steady the stats times shown live. The per-frame matcher
/// can misread a look-alike digit on a single noisy capture frame; ~7 frames at
/// 60fps hides that (~0.1s lag) without noticeably delaying a real change.
const MONITOR_TIME_SMOOTHING_WINDOW: usize = 7;
/// How long after an applied update a newly-connecting client still gets the
/// one-off "plugin updated" notice. Long enough to cover the 1s WebSocket
/// reconnect retry, short enough to avoid a stale notice much later.
const UPDATE_APPLIED_NOTICE_WINDOW: Duration = Duration::from_secs(60);

/// Sliding-window majority vote over the stats times shown live, voting each
/// field (run / target / best) independently so a single-frame digit misread is
/// outvoted before it reaches the UI. The window resets whenever the on-screen
/// level/screen identity changes, so a lone frame of a fast transition is still
/// shown as-is (it simply votes with a window of one).
struct DisplayTimeSmoother {
    key: Option<(Screen, i32, i32, i32)>,
    window: VecDeque<ge::Times>,
}

impl DisplayTimeSmoother {
    fn new() -> Self {
        Self { key: None, window: VecDeque::with_capacity(MONITOR_TIME_SMOOTHING_WINDOW) }
    }

    /// Feeds one frame's reading in and returns the smoothed times to display.
    fn smooth(&mut self, m: &LevelMatch) -> Option<ge::Times> {
        let key = Some((m.screen, m.mission, m.part, m.difficulty));
        if key != self.key {
            self.key = key;
            self.window.clear();
        }
        let Some(times) = m.times else {
            self.window.clear();
            return None;
        };
        if self.window.len() == MONITOR_TIME_SMOOTHING_WINDOW {
            self.window.pop_front();
        }
        self.window.push_back(times);
        Some(ge::Times {
            time: self.majority(|t| t.time)?,
            target_time: self.majority(|t| t.target_time)?,
            best_time: self.majority(|t| t.best_time)?,
        })
    }

    /// Most-common value of `field` across the window, ties to the newest frame.
    fn majority<T: PartialEq + Copy>(&self, field: impl Fn(&ge::Times) -> T) -> Option<T> {
        let mut best: Option<(T, usize)> = None;
        for cand in self.window.iter().rev() {
            let v = field(cand);
            let count = self.window.iter().filter(|t| field(t) == v).count();
            if best.is_none_or(|(_, bc)| count > bc) {
                best = Some((v, count));
            }
        }
        best.map(|(v, _)| v)
    }
}

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

/// A running monitor. OBS pushes captured frames into `mailbox` (keyed by the
/// leaked `producer`); the worker `thread` matches them. Stopping unregisters
/// the callback, closes the mailbox to wake+join the worker, then frees producer.
pub struct MonitorHandle {
    mailbox: Arc<FrameMailbox>,
    producer: ProducerPtr,
    thread: JoinHandle<()>,
    /// The source name this monitor uses, retained in the shared app snapshot.
    source_name: String,
    /// The latched capture transform, shared so a standalone frame dump on the
    /// same source can crop/un-stretch its frames identically to the matcher.
    region: Arc<Mutex<Option<CaptureRegion>>>,
}

/// The leaked `ProducerCtx` pointer, made `Send` so the handle can move to the
/// blocking teardown task. SAFETY: only dereferenced on the OBS graphics thread;
/// freed only after `ge_obs_unregister_frame_callback` ensures no callback runs.
struct ProducerPtr(*mut ProducerCtx);
unsafe impl Send for ProducerPtr {}

/// A captured BGRA frame and its dimensions, owning its pixel buffer. Frames
/// from OBS wrap the C-`malloc`'d buffer the capture bridge returns; test frames
/// own a `Vec`.
struct Frame {
    buf: FrameBuf,
    width: u32,
    height: u32,
    captured_at: Option<Instant>,
    capture_ms: Option<f64>,
    dropped_frames_total: u64,
}

// SAFETY: a `Frame` owns its buffer exclusively and never aliases the raw
// pointer once constructed, so moving it from the producer (graphics) thread to
// the consumer (monitor) thread through the mailbox is sound.
unsafe impl Send for Frame {}

enum FrameBuf {
    /// Buffer handed back by `ge_capture_get_frame`; released with the C `free`.
    CMalloc { ptr: *mut u8, len: usize },
    /// Owned Rust buffer (test fixtures). Only constructed in tests; the OBS
    /// path always uses `CMalloc`.
    #[cfg_attr(not(test), allow(dead_code))]
    Owned(Vec<u8>),
}

impl FrameBuf {
    fn as_slice(&self) -> &[u8] {
        match self {
            // SAFETY: ptr/len describe the single contiguous BGRA buffer this
            // frame owns exclusively until it is dropped.
            FrameBuf::CMalloc { ptr, len } => unsafe { std::slice::from_raw_parts(*ptr, *len) },
            FrameBuf::Owned(bytes) => bytes,
        }
    }
}

impl Drop for FrameBuf {
    fn drop(&mut self) {
        if let FrameBuf::CMalloc { ptr, .. } = *self {
            // SAFETY: allocated by the C capture bridge with malloc; the mailbox
            // owns it exclusively, so it is freed exactly once here.
            unsafe { crate::ffi::free(ptr.cast()) };
        }
    }
}

/// How many captured frames the mailbox buffers. 1 = always match the freshest
/// frame (drop any older unconsumed one); a larger value retains a short backlog.
const FRAME_BUFFER_CAPACITY: usize = 1;

/// A bounded, drop-oldest FIFO frame buffer between the OBS producer and the
/// monitor consumer. Holds up to `capacity` frames; when full, the oldest is
/// dropped/freed so the matcher never falls behind. `capacity == 1` is latest-wins.
struct FrameMailbox {
    /// Maximum number of buffered frames; at least 1.
    capacity: usize,
    state: Mutex<MailboxState>,
    available: Condvar,
}

/// Outcome of a [`FrameMailbox::recv_until`] wait.
enum MailboxRecv {
    Frame(Frame),
    Timeout,
    Closed,
}

struct MailboxState {
    /// Buffered frames, oldest at the front. Capped at `FrameMailbox::capacity`.
    frames: std::collections::VecDeque<Frame>,
    /// Total number of frames dropped because the producer outran the consumer.
    dropped_frames: u64,
    /// Set on stop: wakes a blocked consumer and makes `push` drop new frames.
    closed: bool,
}

impl FrameMailbox {
    fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        FrameMailbox {
            capacity,
            state: Mutex::new(MailboxState {
                frames: std::collections::VecDeque::with_capacity(capacity),
                dropped_frames: 0,
                closed: false,
            }),
            available: Condvar::new(),
        }
    }

    /// Producer: append `frame` to the buffer. When the buffer is full the oldest
    /// frame is dropped (and freed) to make room -- newest always wins. A no-op
    /// once closed.
    fn push(&self, mut frame: Frame) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.closed {
            return; // `frame` is dropped here -> its buffer is freed.
        }
        if state.frames.len() == self.capacity {
            state.frames.pop_front(); // drop the oldest unconsumed frame -> freed.
            state.dropped_frames += 1;
        }
        frame.dropped_frames_total = state.dropped_frames;
        state.frames.push_back(frame);
        drop(state);
        self.available.notify_one();
    }

    /// Consumer: block until a frame is buffered or the mailbox is closed. Returns
    /// the oldest buffered frame, or `None` once closed with nothing left to drain.
    #[cfg(test)]
    fn recv(&self) -> Option<Frame> {
        match self.recv_until(None) {
            MailboxRecv::Frame(frame) => Some(frame),
            MailboxRecv::Closed => None,
            // Unreachable without a deadline; treat as closed rather than panic.
            MailboxRecv::Timeout => None,
        }
    }

    /// Consumer: like [`recv`], but wakes and returns [`MailboxRecv::Timeout`] once
    /// `deadline` passes with no frame. Lets the monitor loop poll the pending-save
    /// timer even while captured frames have stopped (e.g. a paused source).
    fn recv_until(&self, deadline: Option<Instant>) -> MailboxRecv {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            if let Some(frame) = state.frames.pop_front() {
                return MailboxRecv::Frame(frame);
            }
            if state.closed {
                return MailboxRecv::Closed;
            }
            match deadline {
                None => state = self.available.wait(state).unwrap_or_else(|p| p.into_inner()),
                Some(deadline) => {
                    let Some(timeout) = deadline.checked_duration_since(Instant::now()) else {
                        return MailboxRecv::Timeout;
                    };
                    let (next, result) = self.available.wait_timeout(state, timeout).unwrap_or_else(|p| p.into_inner());
                    state = next;
                    if result.timed_out() && state.frames.is_empty() && !state.closed {
                        return MailboxRecv::Timeout;
                    }
                }
            }
        }
    }

    /// Mark the mailbox closed and wake the consumer so its `recv` returns.
    fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.closed = true;
        drop(state);
        self.available.notify_one();
    }
}

/// State the OBS render callback needs to capture a frame and hand it off:
/// capture context, source name, calibrated region, and mailbox. Boxed as the
/// callback `param`; owns the capture context and destroys it on drop.
struct ProducerCtx {
    ctx: *mut crate::ffi::GeCaptureCtx,
    name: CString,
    region: Arc<Mutex<Option<CaptureRegion>>>,
    mailbox: Arc<FrameMailbox>,
    timing_enabled: bool,
}

// SAFETY: see MonitorHandle -- the box is created on the start thread and
// dropped on the stop thread, but `ctx` is only ever used on the graphics thread
// and the two are never concurrent (registration/unregistration fence it).
unsafe impl Send for ProducerCtx {}

impl Drop for ProducerCtx {
    fn drop(&mut self) {
        // Release the GPU surfaces created in `handle_start`. Only reached after
        // the render callback has been unregistered, so `ctx` is unused.
        unsafe { crate::ffi::ge_capture_destroy(self.ctx) };
    }
}

/// OBS render callback: capture one frame of the monitored source and push it
/// into the mailbox. Runs on the graphics thread inside a graphics context, once
/// per rendered frame.
unsafe extern "C" fn ge_frame_callback(param: *mut c_void, _cx: u32, _cy: u32) {
    // SAFETY: `param` is the `ProducerCtx` registered in `handle_start`. OBS
    // serializes this with `ge_obs_unregister_frame_callback`, so it never runs
    // after the monitor unregisters and frees the box.
    let producer = unsafe { &*(param as *const ProducerCtx) };

    // Translate the matcher's learned region (if any) into the C capture
    // transform, so the GPU crops + un-stretches at capture time -- mirrors what
    // the old pull path did per frame.
    let region = {
        let guard = producer.region.lock().unwrap_or_else(|p| p.into_inner());
        guard.map(|r| {
            let out_height = crate::cv::WORK_HEIGHT as u32;
            let out_width = ((out_height as f32 * r.out_aspect).round() as u32).max(1);
            crate::ffi::GeCaptureRegion {
                crop_x: r.crop_x,
                crop_y: r.crop_y,
                crop_w: r.crop_w,
                crop_h: r.crop_h,
                out_width,
                out_height,
            }
        })
    };
    let region_ptr = region.as_ref().map_or(std::ptr::null(), |r| r as *const _);
    let max_height = if region.is_some() { 0 } else { crate::cv::WORK_HEIGHT as u32 };

    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let capture_started = producer.timing_enabled.then(Instant::now);
    // We're already on the graphics thread inside a graphics context, so the
    // obs_enter_graphics nested inside this call is a no-op ref-bump, not a
    // re-lock (OBS tracks the context per thread) -- no deadlock.
    let frame = unsafe {
        crate::ffi::ge_capture_get_frame(
            producer.ctx,
            producer.name.as_ptr(),
            max_height,
            region_ptr,
            &mut width,
            &mut height,
        )
    };
    // Null means no frame this tick: the source wasn't renderable, or (with the
    // double-buffered context) this was the priming call that only stages.
    if frame.is_null() {
        return;
    }
    let (captured_at, capture_ms) = if let Some(capture_started) = capture_started {
        let captured_at = Instant::now();
        (Some(captured_at), Some(captured_at.duration_since(capture_started).as_secs_f64() * 1000.0))
    } else {
        (None, None)
    };
    let len = (width * height * 4) as usize;
    producer.mailbox.push(Frame {
        buf: FrameBuf::CMalloc { ptr: frame, len },
        width,
        height,
        captured_at,
        capture_ms,
        dropped_frames_total: 0,
    });
}

#[derive(Clone, Copy, Debug)]
struct CapturedFrameStats {
    capture_ms: f64,
    mailbox_wait_ms: f64,
    dropped_frames_total: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartParams {
    /// Name of the OBS source to monitor, as reported by `/api/v1/sources`.
    source_name: String,
}

/// Source of frames for fixture-backed monitor-loop tests. The live OBS source
/// uses `ObsSource::capture_with_stats_until` so production can carry timing metadata.
#[cfg(test)]
pub trait FrameSource {
    /// Acquire the next BGRA frame and hand it to `use_frame`. Returns the
    /// closure's value, or `None` when no frame is available right now.
    fn capture<F, R>(&mut self, use_frame: F) -> Option<R>
    where
        F: FnOnce(&[u8], u32, u32) -> R;

    /// Offer the source a capture transform the matcher has learned, so it can
    /// have the GPU crop + un-stretch future frames at capture time. Sources
    /// that can't reshape their frames (test fixtures) ignore it.
    fn set_capture_region(&mut self, _region: Option<CaptureRegion>) {}
}

/// Frame source backed by the live OBS source: consumes the frames the render
/// callback (`ge_frame_callback`) pushes into the shared mailbox. Capture and
/// its GPU surfaces live on the producer side; this only awaits and matches.
struct ObsSource {
    mailbox: Arc<FrameMailbox>,
    /// The calibrated capture transform, shared with the producer callback.
    /// Latched on first sight: fixed for the session, and re-reading after frames
    /// arrive pre-normalized would (incorrectly) clear it.
    region: Arc<Mutex<Option<CaptureRegion>>>,
}

impl ObsSource {
    fn set_capture_region(&mut self, region: Option<CaptureRegion>) {
        // Latch the first transform learned and keep it (see the field comment);
        // the producer callback reads this to crop/un-stretch future captures.
        let mut guard = self.region.lock().unwrap_or_else(|p| p.into_inner());
        if guard.is_none()
            && let Some(r) = region
        {
            tracing::info!(?r, "calibrated capture region; cropping/un-stretching on the GPU");
            *guard = Some(r);
        }
    }

    /// Await the next frame (matching it via `use_frame`), or wake with
    /// [`Captured::Idle`] once `deadline` passes so the caller can poll timers even
    /// while frames have stopped. [`Captured::Closed`] once the mailbox is closed.
    fn capture_with_stats_until<F, R>(&mut self, deadline: Option<Instant>, use_frame: F) -> Captured<R>
    where
        F: FnOnce(&[u8], u32, u32) -> R,
    {
        let frame = match self.mailbox.recv_until(deadline) {
            MailboxRecv::Frame(frame) => frame,
            MailboxRecv::Timeout => return Captured::Idle,
            MailboxRecv::Closed => return Captured::Closed,
        };
        let stats = match (frame.captured_at, frame.capture_ms) {
            (Some(captured_at), Some(capture_ms)) => Some(CapturedFrameStats {
                capture_ms,
                mailbox_wait_ms: captured_at.elapsed().as_secs_f64() * 1000.0,
                dropped_frames_total: frame.dropped_frames_total,
            }),
            _ => None,
        };
        let result = use_frame(frame.buf.as_slice(), frame.width, frame.height);
        Captured::Frame(result, stats)
    }
}

/// Outcome of [`ObsSource::capture_with_stats_until`].
enum Captured<R> {
    /// A frame was matched, with optional capture timing.
    Frame(R, Option<CapturedFrameStats>),
    /// The deadline passed with no frame; poll pending timers and wait again.
    Idle,
    /// The mailbox is closed and drained; the monitor loop should exit.
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MonitorTimingMode {
    Off,
    Slow,
    Verbose,
}

impl MonitorTimingMode {
    fn from_env() -> Self {
        match env::var("GE_MONITOR_TIMING") {
            Ok(value) if matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "slow") => Self::Slow,
            Ok(value) if value.eq_ignore_ascii_case("verbose") => Self::Verbose,
            _ => Self::Off,
        }
    }
}

struct MonitorTiming {
    mode: MonitorTimingMode,
    slow_ms: f64,
    last_dropped_frames_total: u64,
}

impl MonitorTiming {
    fn new(source_fps: f64, mode: MonitorTimingMode) -> Self {
        let frame_ms = if source_fps > 0.0 { 1000.0 / source_fps } else { 16.67 };
        let slow_ms = env::var("GE_MONITOR_SLOW_MS")
            .ok()
            .and_then(|value| f64::from_str(&value).ok())
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or((frame_ms * 2.0).max(40.0));

        Self { mode, slow_ms, last_dropped_frames_total: 0 }
    }

    fn enabled(&self) -> bool {
        self.mode != MonitorTimingMode::Off
    }

    fn observe(
        &mut self,
        stats: Option<CapturedFrameStats>,
        match_ms: Option<f64>,
        cv_runtime_ms: Option<f64>,
        source_fps: f64,
    ) {
        if self.mode == MonitorTimingMode::Off {
            return;
        }
        let (Some(stats), Some(match_ms)) = (stats, match_ms) else {
            return;
        };

        let dropped_frames = stats.dropped_frames_total.saturating_sub(self.last_dropped_frames_total);
        self.last_dropped_frames_total = stats.dropped_frames_total;
        let total_ms = stats.capture_ms + stats.mailbox_wait_ms + match_ms;
        let slow = total_ms >= self.slow_ms || dropped_frames > 0;

        if slow {
            tracing::warn!(
                capture_ms = stats.capture_ms,
                mailbox_wait_ms = stats.mailbox_wait_ms,
                match_ms,
                cv_runtime_ms,
                total_ms,
                dropped_frames,
                dropped_frames_total = stats.dropped_frames_total,
                source_fps,
                slow_threshold_ms = self.slow_ms,
                "monitor frame timing"
            );
        } else if self.mode == MonitorTimingMode::Verbose {
            tracing::info!(
                capture_ms = stats.capture_ms,
                mailbox_wait_ms = stats.mailbox_wait_ms,
                match_ms,
                cv_runtime_ms,
                total_ms,
                dropped_frames,
                dropped_frames_total = stats.dropped_frames_total,
                source_fps,
                slow_threshold_ms = self.slow_ms,
                "monitor frame timing"
            );
        }
    }
}

/// A monitor session: owns the matcher (and its per-resolution scale cache) for
/// one start/stop cycle, so dropping the session clears the cache and each start
/// begins cold. The cache keys on source dimensions, re-learning on resolution changes.
pub struct MonitorSession {
    matcher: CvMatcher,
}

impl MonitorSession {
    /// Builds a session with the given language, using the bundled CV templates
    /// directory resolved at plugin startup.
    pub fn from_env(lang: &str) -> anyhow::Result<Self> {
        let template_dir =
            crate::cv::template_dir().ok_or_else(|| anyhow::anyhow!("CV template directory is not set"))?;
        Self::new(lang, &template_dir)
    }

    /// Builds a session with an explicit language and template directory.
    pub fn new(lang: &str, template_dir: &str) -> anyhow::Result<Self> {
        let matcher = CvMatcher::new(lang, template_dir)
            .map_err(|err| anyhow::anyhow!("failed to init matcher: {}", err.message))?;
        Ok(MonitorSession { matcher })
    }

    pub fn with_diagnostics(mut self, enabled: bool) -> Self {
        self.matcher.set_diagnostics(enabled);
        self
    }

    pub fn set_diagnostics(&mut self, enabled: bool) {
        self.matcher.set_diagnostics(enabled);
    }

    /// Matches one BGRA frame. The matcher's scale cache makes the first overlay
    /// frame at a given resolution costlier (it searches for the scale) and every
    /// later frame at that resolution cheap (it reuses the learned scale).
    pub fn match_frame(&self, bytes: &[u8], width: u32, height: u32) -> opencv::Result<LevelMatch> {
        self.matcher.match_level_from_bgra_bytes(bytes, width, height)
    }

    /// Hot loop used by tests: take each frame `source` yields, match it, and
    /// pass the result to `on_result`.
    #[cfg(test)]
    pub fn run<S, F>(&self, source: &mut S, mut on_result: F)
    where
        S: FrameSource,
        F: FnMut(opencv::Result<LevelMatch>),
    {
        while let Some(result) = source.capture(|bytes, w, h| self.match_frame(bytes, w, h)) {
            // Once the matcher has calibrated this source's aspect, hand the
            // transform to the capture layer so subsequent frames are cropped +
            // un-stretched on the GPU at capture time.
            source.set_capture_region(self.matcher.capture_region());
            on_result(result);
        }
    }
}

fn handle_detected_language(
    info: &LevelMatch,
    session: &mut MonitorSession,
    active_lang: &mut String,
    language_notified: &mut bool,
    event_tx: &broadcast::Sender<MonitorEvent>,
    make_session: impl FnOnce(&str) -> anyhow::Result<MonitorSession>,
) -> bool {
    let Some(detected_lang) = info.detected_lang.as_deref().map(str::to_owned) else {
        return false;
    };

    if detected_lang == *active_lang {
        if !*language_notified {
            tracing::info!(detected_lang, "detected ROM language");
            *language_notified = true;
            let _ = event_tx.send(MonitorEvent::LanguageDetected { lang: active_lang.clone() });
        }
        return false;
    }

    tracing::info!(
        active_lang = %active_lang,
        detected_lang,
        "detected ROM language; switching monitor templates"
    );
    match make_session(&detected_lang) {
        Ok(next_session) => {
            *session = next_session;
            *active_lang = detected_lang.clone();
        }
        Err(err) => {
            tracing::error!(detected_lang, "failed to switch monitor language after detection: {err}");
            return false;
        }
    }

    // A real language switch is always worth re-notifying, even if we already
    // notified for the previous language this session.
    *language_notified = true;
    let _ = event_tx.send(MonitorEvent::LanguageDetected { lang: active_lang.clone() });

    true
}

fn log_level_match(info: &LevelMatch) {
    match serde_json::to_string(info) {
        Ok(json) => tracing::info!("{json}"),
        Err(err) => tracing::info!(?info, "failed to serialize level match as JSON: {err}"),
    }
}

#[axum::debug_handler]
pub async fn handle_start(State(state): State<AppState>, Json(params): Json<StartParams>) -> Result<impl IntoResponse> {
    // Keep the original source name for the app snapshot; it is also converted
    // to a CString below for the C capture bridge.
    let status_source_name = params.source_name.clone();
    let recording_options = state.settings.get_recording_options();
    let source_name =
        CString::new(params.source_name).map_err(|_| (StatusCode::BAD_REQUEST, "source name contains a null byte"))?;

    // Only one monitor may run at a time; reject the request if one already is.
    let mut guard = state.monitor.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_some() {
        return Err((StatusCode::CONFLICT, "a monitor is already running").into());
    }

    if !crate::recording::ensure_replay_buffer_running() {
        return Err((StatusCode::PRECONDITION_FAILED, "replay buffer is unavailable").into());
    }
    state.recording_state.clear();

    // Build the session (and its fresh, empty scale cache) up front so any
    // configuration error surfaces as a failed request rather than a thread that
    // silently exits.
    let session = MonitorSession::from_env(DEFAULT_MONITOR_LANGUAGE).map_err(|err| {
        tracing::error!("failed to start monitor: {err}");
        (StatusCode::INTERNAL_SERVER_ERROR, "failed to init matcher")
    })?;

    // Reusable capture context (and GPU surfaces), created once per session and
    // destroyed with the ProducerCtx on stop. Double-buffered so readback pipelines
    // without stalling OBS's render; the first frame only primes and yields none.
    let ctx = unsafe { crate::ffi::ge_capture_create(true) };
    if ctx.is_null() {
        tracing::error!("failed to create capture context; monitor not started");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "failed to create capture context").into());
    }

    // Shared between the OBS producer (render callback) and the worker consumer:
    // the frame mailbox and latched capture region. Capacity 1 is drop-oldest
    // (freshest frame only); raise it to retain a short backlog.
    let mailbox = Arc::new(FrameMailbox::new(FRAME_BUFFER_CAPACITY));
    let region = Arc::new(Mutex::new(None));
    let monitor_timing_mode = MonitorTimingMode::from_env();

    // Producer state handed to OBS as the render-callback param. Boxed and leaked
    // to a raw pointer for the monitor's lifetime; reclaimed (and the capture
    // context destroyed) in `handle_stop`.
    let producer = Box::into_raw(Box::new(ProducerCtx {
        ctx,
        name: source_name,
        region: region.clone(),
        mailbox: mailbox.clone(),
        timing_enabled: monitor_timing_mode != MonitorTimingMode::Off,
    }));

    // From here on OBS pushes a captured frame into the mailbox once per rendered
    // frame -- the push model that replaces the old capture-in-a-spin-loop.
    unsafe { crate::ffi::ge_obs_register_frame_callback(ge_frame_callback, producer.cast()) };

    // Run the matcher on a dedicated OS thread so its blocking, CPU-bound work
    // never ties up the async runtime's worker threads. The session is moved
    // onto the thread and dropped when the loop exits, clearing the cache.
    let worker_mailbox = mailbox.clone();
    // Retain each new display match in the app snapshot. We dedup here so the
    // snapshot only changes when the matched state changes (ignoring runtime_ms),
    // rather than every frame.
    let snapshot = state.snapshot.clone();
    // Handed to the recorder so it can broadcast a `RecordingSaved` event once a
    // run's clip is written out of the replay buffer.
    let event_tx = state.event_tx.clone();
    let recording_state = state.recording_state.clone();
    let monitor_annotations_state = state.clone();
    let recording_source_name = status_source_name.clone();
    let recording_lang = DEFAULT_MONITOR_LANGUAGE.to_owned();
    let source_fps = unsafe { crate::ffi::ge_obs_video_fps() };
    // Kept for the handle so a standalone frame dump can share the latched region.
    let handle_region = region.clone();
    let thread = std::thread::Builder::new().name("ge-monitor".to_owned()).spawn(move || {
        let mut source = ObsSource { mailbox: worker_mailbox, region };
        let mut session = session;
        let mut active_lang = recording_lang.clone();
        let mut language_notified = false;
        let mut last: Option<LevelMatch> = None;
        let mut display_smoother = DisplayTimeSmoother::new();
        let mut last_diagnostics_enabled = false;
        let mut last_fps_emit = Instant::now();
        let mut last_frame_completed: Option<Instant> = None;
        let mut slowest_frame_fps: Option<f64> = None;
        let mut monitor_timing = MonitorTiming::new(source_fps, monitor_timing_mode);
        let timing_enabled = monitor_timing.enabled();
        // Drives the replay-buffer save/trim as the session progresses. Fed
        // every matched frame (not just state changes) so its save timer is
        // polled each tick.
        let mut recording = crate::recording::RecordingState::new(
            event_tx.clone(),
            recording_state,
            recording_options,
            recording_source_name,
            recording_lang,
        );
        loop {
            let diagnostics_enabled = monitor_annotations_state.monitor_annotations_enabled.load(Ordering::Acquire);
            if diagnostics_enabled != last_diagnostics_enabled {
                last_diagnostics_enabled = diagnostics_enabled;
                last = None;
            }
            session.set_diagnostics(diagnostics_enabled);
            // Wake by the pending save's fire time even if no frame arrives, so a
            // paused/stalled source can't stall (and eventually roll out of the
            // replay buffer) a scheduled save.
            let deadline = recording.pending_fire_at();
            let (result, match_ms, stats) = match source.capture_with_stats_until(deadline, |bytes, w, h| {
                if timing_enabled {
                    let match_started = Instant::now();
                    let result = session.match_frame(bytes, w, h);
                    let match_ms = match_started.elapsed().as_secs_f64() * 1000.0;
                    (result, Some(match_ms))
                } else {
                    (session.match_frame(bytes, w, h), None)
                }
            }) {
                Captured::Frame((result, match_ms), stats) => (result, match_ms, stats),
                Captured::Idle => {
                    recording.poll_pending(Instant::now());
                    continue;
                }
                Captured::Closed => break,
            };
            let now = Instant::now();
            if let Some(previous) = last_frame_completed {
                let frame_elapsed = now.duration_since(previous).as_secs_f64();
                if frame_elapsed > 0.0 {
                    let frame_fps = 1.0 / frame_elapsed;
                    slowest_frame_fps = Some(slowest_frame_fps.map_or(frame_fps, |fps| fps.min(frame_fps)));
                }
            }
            last_frame_completed = Some(now);

            if now.duration_since(last_fps_emit) >= MONITOR_FPS_EMIT_INTERVAL {
                if let Some(processed_fps) = slowest_frame_fps {
                    let _ = event_tx.send(MonitorEvent::MonitorFps(MonitorFps { processed_fps, source_fps }));
                }
                last_fps_emit = now;
                slowest_frame_fps = None;
            }

            // Once the matcher has calibrated this source's aspect, hand the
            // transform to the capture layer so subsequent frames are cropped +
            // un-stretched on the GPU at capture time.
            source.set_capture_region(session.matcher.capture_region());

            match result {
                Ok(info) => {
                    monitor_timing.observe(stats, match_ms, Some(info.runtime_ms), source_fps);
                    tracing::debug!(?info);
                    if handle_detected_language(
                        &info,
                        &mut session,
                        &mut active_lang,
                        &mut language_notified,
                        &event_tx,
                        |lang| Ok(MonitorSession::from_env(lang)?.with_diagnostics(diagnostics_enabled)),
                    ) {
                        recording.set_rom_language(active_lang.clone());
                        last = None;
                    }

                    // The recorder votes over raw per-frame readings itself, so it
                    // must see the unsmoothed match; only the live display is voted.
                    recording.on_frame(now, &info);
                    let mut display = info;
                    display.times = display_smoother.smooth(&display);
                    let changed = last.as_ref().is_none_or(|prev| !prev.same_state(&display));
                    if changed {
                        log_level_match(&display);
                        last = Some(display.clone());
                        snapshot.set_match(Some(display));
                    }
                }
                Err(e) => {
                    monitor_timing.observe(stats, match_ms, None, source_fps);
                    tracing::error!("err: {}", e.message);
                }
            }
        }
        tracing::info!("monitor loop exiting");
    });
    let thread = match thread {
        Ok(thread) => thread,
        Err(err) => {
            tracing::error!("failed to spawn monitor thread: {err}");
            // Unwind the registration and free the producer (which destroys ctx).
            unsafe { crate::ffi::ge_obs_unregister_frame_callback(ge_frame_callback, producer.cast()) };
            drop(unsafe { Box::from_raw(producer) });
            return Err((StatusCode::INTERNAL_SERVER_ERROR, "failed to spawn monitor thread").into());
        }
    };

    *guard = Some(MonitorHandle {
        mailbox,
        producer: ProducerPtr(producer),
        thread,
        source_name: status_source_name.clone(),
        region: handle_region,
    });
    state.snapshot.set_monitor_running(status_source_name);
    state.snapshot.set_replay_buffer(crate::http::current_replay_buffer_status());
    tracing::info!("monitor started");

    Ok(StatusCode::OK)
}

#[axum::debug_handler]
pub async fn handle_stop(State(state): State<AppState>) -> Result<impl IntoResponse> {
    if !stop_monitor(&state).await {
        return Err((StatusCode::CONFLICT, "no monitor is running").into());
    }
    let _ = state.event_tx.send(MonitorEvent::MonitorStopped { reason: MonitorStoppedReason::UserStopped });

    Ok(StatusCode::OK)
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

/// Stop the active monitor, if any, and clear all retained monitor/recording
/// state. Returns `false` when no monitor was running.
pub(crate) async fn stop_monitor(state: &AppState) -> bool {
    let handle = {
        let mut guard = state.monitor.lock().unwrap_or_else(|p| p.into_inner());
        guard.take()
    };

    let Some(handle) = handle else {
        return false;
    };

    // Tear down on a blocking thread so we don't stall the async runtime while
    // the in-flight match finishes. Joining the thread drops the session,
    // releasing the matcher and its scale cache.
    tokio::task::spawn_blocking(move || {
        // Destructure up front so the closure captures the Send `ProducerPtr`
        // field, not the inner raw pointer (disjoint closure capture would reach
        // through a `ProducerPtr(producer)` pattern). Unwrap it as a local after.
        let MonitorHandle { mailbox, producer, thread, .. } = handle;
        let producer = producer.0;
        // Stop new frames first. `ge_obs_unregister_frame_callback` serializes with
        // callback invocation, so once it returns the callback is neither running
        // nor will run again -- the ProducerCtx is then safe to free below.
        unsafe { crate::ffi::ge_obs_unregister_frame_callback(ge_frame_callback, producer.cast()) };
        // Wake the worker out of its blocking `recv` so the run loop exits.
        mailbox.close();
        if thread.join().is_err() {
            tracing::error!("monitor thread panicked");
        }
        // Worker is done and no callback can fire: reclaim the producer, whose
        // Drop destroys the capture context.
        drop(unsafe { Box::from_raw(producer) });
    })
    .await
    .ok();

    // Clear retained monitor/match/recording state so all clients receive one
    // backend-owned snapshot reflecting the stopped session.
    state.snapshot.set_monitor_stopped();
    state.recording_state.clear();

    if state.settings.get().stop_replay_buffer_when_monitor_stopped {
        crate::recording::stop_replay_buffer_if_active();
        state.snapshot.set_replay_buffer(crate::http::current_replay_buffer_status());
    }

    tracing::info!("monitor stopped");

    true
}

/// Upgrades the connection to a WebSocket streaming [`MonitorEvent`]s as JSON.
/// The complete retained app snapshot is sent on connect and after every
/// retained-state change; one-off events such as `recordingSaved` are forwarded as they occur.
pub async fn handle_ws(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut snapshots = state.snapshot.subscribe();
    let mut events = state.event_tx.subscribe();

    // Announce which build serves this API first, so a stale tab can reload
    // before it starts acting on snapshot or one-off events.
    let version = MonitorEvent::Version { build_id: super::index::BUILD_ID.clone() };
    if send_event(&mut socket, &version).await.is_err() {
        return;
    }

    if send_current_snapshot(&mut socket, &mut snapshots).await.is_err() {
        return;
    }

    // A one-off "plugin updated" notice for any client connecting shortly after
    // this core was loaded via an applied update (not a cold start or rollback),
    // bounded by a grace period so late-connecting clients don't see it stale.
    if state.reloaded_at.is_some_and(|when| when.elapsed() < UPDATE_APPLIED_NOTICE_WINDOW) {
        let settings = state.settings.get();
        let release_url = if settings.last_known_update_version.as_deref() == Some(crate::PLUGIN_VERSION) {
            settings.last_known_update_release_url
        } else {
            None
        };
        let applied = MonitorEvent::UpdateApplied { version: crate::PLUGIN_VERSION.to_owned(), release_url };
        if send_event(&mut socket, &applied).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
            changed = snapshots.changed() => {
                if changed.is_err() {
                    break;
                }
                if send_current_snapshot(&mut socket, &mut snapshots).await.is_err() {
                    break;
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        if send_event(&mut socket, &event).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(_)) => {}
                    _ => break,
                }
            }
        }
    }
}

/// Serializes `event` to JSON and sends it over `socket`. A serialization error
/// is swallowed (the event is skipped); a transport error propagates so the
/// caller can drop the connection.
async fn send_event(socket: &mut WebSocket, event: &MonitorEvent) -> Result<(), axum::Error> {
    if let Ok(text) = serde_json::to_string(event) {
        socket.send(Message::Text(text.into())).await?;
    }
    Ok(())
}

/// Sends the current retained app snapshot, marking the watch value as seen.
async fn send_current_snapshot(
    socket: &mut WebSocket,
    rx: &mut watch::Receiver<crate::http::AppSnapshot>,
) -> Result<(), axum::Error> {
    let state = Box::new(rx.borrow_and_update().clone());
    send_event(socket, &MonitorEvent::Snapshot { state }).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use opencv::prelude::*;
    use opencv::{imgcodecs, imgproc};

    use super::*;
    use crate::ge::Times;

    // Builds the classified stats-screen times for a test expectation.
    const fn times(time: i32, target_time: Option<i32>, best_time: Option<i32>) -> Option<Times> {
        Some(Times { time, target_time, best_time })
    }

    // Templates ship alongside obs2/; screenshots live under test/screenshots-*.
    const TEMPLATES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../cv_templates");
    const SCREENSHOTS_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test");

    /// Decodes a screenshot into a contiguous BGRA byte buffer plus dimensions,
    /// matching the layout OBS hands the matcher.
    fn load_bgra(rel_path: &str) -> (Vec<u8>, u32, u32) {
        let path = format!("{SCREENSHOTS_ROOT}/{rel_path}");
        let bgr = imgcodecs::imread(&path, imgcodecs::IMREAD_COLOR).expect("imread");
        assert!(!bgr.empty(), "could not read {path}");
        let mut bgra = Mat::default();
        imgproc::cvt_color_def(&bgr, &mut bgra, imgproc::COLOR_BGR2BGRA).expect("cvt");
        let (w, h) = (bgra.cols() as u32, bgra.rows() as u32);
        let bytes = bgra.data_bytes().expect("data_bytes").to_vec();
        (bytes, w, h)
    }

    // A minimal stats-screen match for the display smoother; only the identity
    // fields and `times` matter to it.
    fn stats_frame(mission: i32, times: Option<Times>) -> LevelMatch {
        LevelMatch {
            screen: Screen::Stats,
            mission,
            part: 1,
            difficulty: 2,
            detected_lang: None,
            times,
            raw_times: Vec::new(),
            match_regions: Vec::new(),
            annotation_sets: Vec::new(),
            runtime_ms: 0.0,
        }
    }

    #[test]
    fn display_smoother_outvotes_a_single_frame_best_time_flicker() {
        let mut smoother = DisplayTimeSmoother::new();
        let mut out = None;
        // 28 is stable apart from a lone 20 flicker; the majority holds even on the
        // flicker frame and at the end (whose last frame also read 20).
        for best in [Some(28), Some(28), Some(20), Some(28), Some(28), Some(20)] {
            out = smoother.smooth(&stats_frame(1, times(28, Some(300), best)));
        }
        assert_eq!(out, times(28, Some(300), Some(28)));
    }

    #[test]
    fn display_smoother_passes_a_lone_frame_through() {
        // A fast transition may only ever yield one frame; it is shown as read.
        let mut smoother = DisplayTimeSmoother::new();
        let out = smoother.smooth(&stats_frame(1, times(28, Some(300), Some(20))));
        assert_eq!(out, times(28, Some(300), Some(20)));
    }

    #[test]
    fn display_smoother_resets_on_level_change() {
        let mut smoother = DisplayTimeSmoother::new();
        for _ in 0..4 {
            smoother.smooth(&stats_frame(1, times(28, Some(300), Some(28))));
        }
        // A different level's window must start fresh, not inherit the old votes.
        let out = smoother.smooth(&stats_frame(2, times(50, Some(300), Some(40))));
        assert_eq!(out, times(50, Some(300), Some(40)));
    }

    #[test]
    fn display_smoother_votes_each_field_independently() {
        let mut smoother = DisplayTimeSmoother::new();
        let mut out = None;
        // Run time flickers while best/target stay put: only the run time is voted.
        for time in [28, 28, 61, 28, 28] {
            out = smoother.smooth(&stats_frame(1, times(time, Some(300), Some(28))));
        }
        assert_eq!(out, times(28, Some(300), Some(28)));
    }

    /// Frame source that replays decoded fixtures, returning `None` once the
    /// stream is exhausted so a `run` loop exits.
    struct FixtureSource {
        frames: Vec<(Vec<u8>, u32, u32)>,
        idx: usize,
    }

    impl FrameSource for FixtureSource {
        fn capture<F, R>(&mut self, use_frame: F) -> Option<R>
        where
            F: FnOnce(&[u8], u32, u32) -> R,
        {
            let (bytes, w, h) = self.frames.get(self.idx)?;
            self.idx += 1;
            Some(use_frame(bytes, *w, *h))
        }
    }

    struct Case {
        file: &'static str,
        lang: &'static str,
        mission: i32,
        part: i32,
        difficulty: i32,
        times: Option<Times>,
    }

    // Expected matches spanning both capture resolutions (av2hdmi 640x480,
    // emu 1440x1080), both languages, and both overlay screens (level-start
    // briefing -> no times; post-mission stats -> times).
    const CASES: &[Case] = &[
        Case {
            file: "screenshots-av2hdmi/en - start - 08 - Agent.png",
            lang: "en",
            mission: 5,
            part: 1,
            difficulty: 0,
            times: None,
        },
        Case {
            file: "screenshots-av2hdmi/en - start - 16 - Secret Agent.png",
            lang: "en",
            mission: 7,
            part: 2,
            difficulty: 1,
            times: None,
        },
        Case {
            // Dam on Agent; Dam's target is set for Secret Agent, so no target
            // row shows here -- the second time is the best time.
            file: "screenshots-av2hdmi/en - stats - 01 - Agent - 0119_0119.png",
            lang: "en",
            mission: 1,
            part: 1,
            difficulty: 0,
            times: times(79, None, Some(79)),
        },
        Case {
            // Archives on Agent; its target is set for 00 Agent, so no target row.
            file: "screenshots-av2hdmi/en - stats - 11 - Agent - 0043_0043.png",
            lang: "en",
            mission: 6,
            part: 2,
            difficulty: 0,
            times: times(43, None, Some(43)),
        },
        Case {
            file: "screenshots-emu/en - start - 20 - Agent.png",
            lang: "en",
            mission: 9,
            part: 1,
            difficulty: 0,
            times: None,
        },
        Case {
            // Runway on Agent; its target IS set for Agent, so the target row
            // shows (middle time), followed by the best time.
            file: "screenshots-emu/en - stats - 03 - Agent - 0033_0500_0033.png",
            lang: "en",
            mission: 1,
            part: 3,
            difficulty: 0,
            times: times(33, Some(300), Some(33)),
        },
        Case {
            file: "screenshots-emu/jp - start - 01 - 00 Agent.png",
            lang: "jp",
            mission: 1,
            part: 1,
            difficulty: 2,
            times: None,
        },
        Case {
            // Dam on Agent (jp); target is Secret Agent, so no target row.
            file: "screenshots-emu/jp - stats - 01 - Agent - 0137_0137.png",
            lang: "jp",
            mission: 1,
            part: 1,
            difficulty: 0,
            times: times(97, None, Some(97)),
        },
    ];

    fn assert_case(session: &MonitorSession, case: &Case) {
        let (bytes, w, h) = load_bgra(case.file);
        let m = session.match_frame(&bytes, w, h).expect("match");
        assert_eq!(m.mission, case.mission, "{} mission", case.file);
        assert_eq!(m.part, case.part, "{} part", case.file);
        assert_eq!(m.difficulty, case.difficulty, "{} difficulty", case.file);
        assert_eq!(m.times, case.times, "{} times", case.file);
    }

    #[test]
    fn matches_known_frames() {
        for case in CASES {
            let session = MonitorSession::new(case.lang, TEMPLATES_DIR).expect("session");
            assert_case(&session, case);
        }
    }

    #[test]
    fn start_screen_language_mismatch_is_detected_and_rejected() {
        let cases = [
            ("jp", "en", "screenshots-emu/en - start - 01 - Agent.png"),
            ("en", "jp", "screenshots-emu/jp - start - 01 - Agent.png"),
            ("jp", "en", "screenshots-av2hdmi/en - start - 3 - 00 Agent - blackbars.png"),
        ];

        for (configured, detected, file) in cases {
            let session = MonitorSession::new(configured, TEMPLATES_DIR).expect("session");
            let (bytes, w, h) = load_bgra(file);
            let m = session.match_frame(&bytes, w, h).expect("match");
            assert_eq!(m.detected_lang.as_deref(), Some(detected), "{file} detected language");
            assert_eq!(m.screen, crate::cv::Screen::Unknown, "{file} screen");
            assert_eq!(m.raw_times, Vec::<i32>::new(), "{file} raw times");
            assert_eq!(m.times, None, "{file} times");
        }
    }

    #[test]
    fn detected_language_switches_active_monitor_language_and_notifies_once() {
        let mut session = MonitorSession::new("en", TEMPLATES_DIR).expect("session");
        let mut active_lang = "en".to_owned();
        let mut language_notified = false;
        let (event_tx, mut event_rx) = broadcast::channel(8);

        let (start_b, start_w, start_h) = load_bgra("screenshots-emu/jp - start - 01 - Agent.png");
        let mismatch = session.match_frame(&start_b, start_w, start_h).expect("mismatch match");
        assert_eq!(mismatch.detected_lang.as_deref(), Some("jp"));
        assert_eq!(mismatch.screen, crate::cv::Screen::Unknown);

        let switched = handle_detected_language(
            &mismatch,
            &mut session,
            &mut active_lang,
            &mut language_notified,
            &event_tx,
            |lang| MonitorSession::new(lang, TEMPLATES_DIR),
        );

        assert!(switched, "mismatch should switch the active matcher");
        assert_eq!(active_lang, "jp");
        assert!(language_notified);

        let event = event_rx.try_recv().expect("language detected event");
        assert!(matches!(event, MonitorEvent::LanguageDetected { lang } if lang == "jp"));

        let (stats_b, stats_w, stats_h) = load_bgra("screenshots-emu/jp - stats - 01 - Agent - 0137_0137.png");
        let stats = session.match_frame(&stats_b, stats_w, stats_h).expect("jp stats after switch");
        assert_eq!(stats.screen, crate::cv::Screen::Stats);
        assert_eq!(stats.mission, 1);
        assert_eq!(stats.part, 1);
        assert_eq!(stats.difficulty, 0);
        assert_eq!(stats.times, times(97, None, Some(97)));

        let repeated = handle_detected_language(
            &mismatch,
            &mut session,
            &mut active_lang,
            &mut language_notified,
            &event_tx,
            |lang| MonitorSession::new(lang, TEMPLATES_DIR),
        );
        assert!(!repeated, "already-active detected language should not switch again");
        assert!(matches!(event_rx.try_recv(), Err(broadcast::error::TryRecvError::Empty)));
    }

    #[test]
    fn detected_language_notifies_when_already_active() {
        let mut session = MonitorSession::new("en", TEMPLATES_DIR).expect("session");
        let mut active_lang = "en".to_owned();
        let mut language_notified = false;
        let (event_tx, mut event_rx) = broadcast::channel(8);

        let (start_b, start_w, start_h) = load_bgra("screenshots-emu/en - start - 01 - Agent.png");
        let detected = session.match_frame(&start_b, start_w, start_h).expect("detected match");
        assert_eq!(detected.detected_lang.as_deref(), Some("en"));

        let switched = handle_detected_language(
            &detected,
            &mut session,
            &mut active_lang,
            &mut language_notified,
            &event_tx,
            |lang| MonitorSession::new(lang, TEMPLATES_DIR),
        );

        assert!(!switched, "already-active detected language should not switch");
        assert_eq!(active_lang, "en");
        assert!(language_notified);
        let event = event_rx.try_recv().expect("language detected event");
        assert!(matches!(event, MonitorEvent::LanguageDetected { lang } if lang == "en"));
    }

    #[test]
    fn detected_language_can_switch_more_than_once_per_monitor_session() {
        let mut session = MonitorSession::new("en", TEMPLATES_DIR).expect("session");
        let mut active_lang = "en".to_owned();
        let mut language_notified = false;
        let (event_tx, mut event_rx) = broadcast::channel(8);

        let (en_b, en_w, en_h) = load_bgra("screenshots-emu/en - start - 01 - Agent.png");
        let en_detected = session.match_frame(&en_b, en_w, en_h).expect("en match");
        assert_eq!(en_detected.detected_lang.as_deref(), Some("en"));

        let first = handle_detected_language(
            &en_detected,
            &mut session,
            &mut active_lang,
            &mut language_notified,
            &event_tx,
            |lang| MonitorSession::new(lang, TEMPLATES_DIR),
        );
        assert!(!first, "initial same-language detection should not switch");
        assert_eq!(active_lang, "en");
        assert!(language_notified);
        let event = event_rx.try_recv().expect("language detected event");
        assert!(matches!(event, MonitorEvent::LanguageDetected { lang } if lang == "en"));

        let (jp_b, jp_w, jp_h) = load_bgra("screenshots-emu/jp - start - 01 - Agent.png");
        let jp_mismatch = session.match_frame(&jp_b, jp_w, jp_h).expect("jp mismatch match");
        assert_eq!(jp_mismatch.detected_lang.as_deref(), Some("jp"));

        let switched_to_jp = handle_detected_language(
            &jp_mismatch,
            &mut session,
            &mut active_lang,
            &mut language_notified,
            &event_tx,
            |lang| MonitorSession::new(lang, TEMPLATES_DIR),
        );
        assert!(switched_to_jp, "language change should switch after notification");
        assert_eq!(active_lang, "jp");
        let event = event_rx.try_recv().expect("language detected event on switch");
        assert!(matches!(event, MonitorEvent::LanguageDetected { lang } if lang == "jp"));

        let en_mismatch = session.match_frame(&en_b, en_w, en_h).expect("en mismatch match");
        assert_eq!(en_mismatch.detected_lang.as_deref(), Some("en"));

        let switched_back_to_en = handle_detected_language(
            &en_mismatch,
            &mut session,
            &mut active_lang,
            &mut language_notified,
            &event_tx,
            |lang| MonitorSession::new(lang, TEMPLATES_DIR),
        );
        assert!(switched_back_to_en, "a second language change should still switch");
        assert_eq!(active_lang, "en");
        let event = event_rx.try_recv().expect("language detected event on switch back");
        assert!(matches!(event, MonitorEvent::LanguageDetected { lang } if lang == "en"));
    }

    #[test]
    fn cache_is_consistent_and_per_session() {
        // 640x480 and 1440x1080 frames, both stats screens with known times.
        let dam = "screenshots-av2hdmi/en - stats - 01 - Agent - 0119_0119.png"; // [79,79]
        let runway = "screenshots-emu/en - stats - 03 - Agent - 0033_0500_0033.png"; // [33,300,33]
        let (dam_b, dam_w, dam_h) = load_bgra(dam);
        let (run_b, run_w, run_h) = load_bgra(runway);

        let session = MonitorSession::new("en", TEMPLATES_DIR).expect("session");

        // First (cold) and second (warm, cache hit) reads of the same frame must
        // agree -- the cached scale must not change the result.
        let cold = session.match_frame(&dam_b, dam_w, dam_h).expect("cold");
        let warm = session.match_frame(&dam_b, dam_w, dam_h).expect("warm");
        assert_eq!(cold.times, times(79, None, Some(79)));
        assert_eq!(warm.times, cold.times);
        assert_eq!((warm.mission, warm.part), (cold.mission, cold.part));

        // A different resolution in the same session is keyed separately, so the
        // 480p cache never corrupts the 1080p read, and vice versa.
        let other = session.match_frame(&run_b, run_w, run_h).expect("other res");
        assert_eq!(other.times, times(33, Some(300), Some(33)));
        let back = session.match_frame(&dam_b, dam_w, dam_h).expect("back");
        assert_eq!(back.times, times(79, None, Some(79)));

        // A fresh session starts cold and reproduces the result exactly,
        // confirming the cache is owned per-session (cleared on stop).
        let session2 = MonitorSession::new("en", TEMPLATES_DIR).expect("session2");
        let fresh = session2.match_frame(&dam_b, dam_w, dam_h).expect("fresh");
        assert_eq!(fresh.times, times(79, None, Some(79)));
    }

    #[test]
    fn run_processes_a_frame_stream_until_exhausted() {
        let files = [
            "screenshots-emu/en - start - 20 - Agent.png",
            "screenshots-emu/en - stats - 03 - Agent - 0033_0500_0033.png",
            "screenshots-av2hdmi/en - start - 08 - Agent.png",
        ];
        let frames: Vec<_> = files.iter().map(|f| load_bgra(f)).collect();

        let mut source = FixtureSource { frames, idx: 0 };
        let session = MonitorSession::new("en", TEMPLATES_DIR).expect("session");

        let mut results = Vec::new();
        session.run(&mut source, |r| results.push(r.expect("match")));

        assert_eq!(results.len(), 3, "every fixture frame is processed once");
        assert_eq!(results[0].mission, 9); // start 20 -> Egyptian
        assert_eq!(results[1].times, times(33, Some(300), Some(33))); // stats 03 (Runway on Agent: run, target, best)
        assert_eq!(results[2].mission, 5); // start 08 -> Surface 2
    }

    fn owned_frame(tag: u8, width: u32) -> Frame {
        Frame {
            buf: FrameBuf::Owned(vec![tag]),
            width,
            height: 1,
            captured_at: None,
            capture_ms: None,
            dropped_frames_total: 0,
        }
    }

    #[test]
    fn mailbox_capacity_one_keeps_only_the_latest_frame() {
        let mailbox = FrameMailbox::new(1);
        // Two pushes with no intervening recv: at capacity 1 the newer frame
        // evicts (and frees) the older one, so only the latest is delivered.
        mailbox.push(owned_frame(1, 10));
        mailbox.push(owned_frame(2, 20));
        let frame = mailbox.recv().expect("a frame is buffered");
        assert_eq!(frame.width, 20, "newest frame wins");
        assert_eq!(frame.buf.as_slice(), &[2]);
    }

    #[test]
    fn mailbox_buffers_up_to_capacity_then_drops_oldest() {
        let mailbox = FrameMailbox::new(2);
        // Within capacity, frames are retained and delivered oldest-first.
        mailbox.push(owned_frame(1, 10));
        mailbox.push(owned_frame(2, 20));
        // A third push overflows: the oldest (frame 1) is dropped.
        mailbox.push(owned_frame(3, 30));
        assert_eq!(mailbox.recv().expect("first").width, 20, "oldest survivor first");
        assert_eq!(mailbox.recv().expect("second").width, 30, "then the newest");
    }

    #[test]
    fn mailbox_recv_returns_none_once_closed_and_drained() {
        let mailbox = FrameMailbox::new(1);
        // A frame still buffered at close is drained before recv reports closed.
        mailbox.push(owned_frame(7, 30));
        mailbox.close();
        assert_eq!(mailbox.recv().expect("drains the buffered frame").width, 30);
        assert!(mailbox.recv().is_none(), "closed and drained -> None");
        // A push after close is dropped, not stored.
        mailbox.push(owned_frame(9, 40));
        assert!(mailbox.recv().is_none(), "push after close is a no-op");
    }
}
