use std::ffi::{CString, c_void};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use axum::Json;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response, Result};
use serde::Deserialize;
use tokio::sync::{broadcast, watch};

use crate::cv::{CaptureRegion, CvMatcher, LevelMatch};
use crate::http::{AppState, MonitorEvent};

/// A running monitor. OBS pushes captured frames into `mailbox` from its render
/// callback (keyed by the leaked `producer` pointer); the worker `thread`
/// consumes and matches them. Stopping unregisters the callback, closes the
/// mailbox to wake the worker, joins it, then frees the producer.
pub struct MonitorHandle {
    mailbox: Arc<FrameMailbox>,
    producer: ProducerPtr,
    thread: JoinHandle<()>,
    /// The source name and language this monitor was started with, retained so
    /// `/api/v1/monitor/status` can report what is currently being monitored.
    source_name: String,
    lang: String,
}

/// The leaked `ProducerCtx` pointer, made `Send` so the handle can move to the
/// blocking teardown task.
///
/// SAFETY: the pointer is only dereferenced from the OBS graphics thread (the
/// render callback). The start thread creates it and the stop thread frees it,
/// but only after `ge_obs_unregister_frame_callback` guarantees no callback is
/// running -- so it is never aliased across threads concurrently.
struct ProducerPtr(*mut ProducerCtx);
unsafe impl Send for ProducerPtr {}

/// A captured BGRA frame and its dimensions, owning its pixel buffer. Frames
/// from OBS wrap the C-`malloc`'d buffer the capture bridge returns; test frames
/// own a `Vec`.
struct Frame {
    buf: FrameBuf,
    width: u32,
    height: u32,
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

/// A bounded, drop-oldest frame buffer between the OBS producer and the monitor
/// consumer. Holds at most `capacity` frames; when full, the oldest unconsumed
/// frame is dropped (and freed) to make room for the newest, so the matcher never
/// falls behind -- when processing is slower than the frame rate the surplus
/// frames are discarded rather than queued unboundedly. Frames are delivered
/// oldest-first (FIFO). At `capacity == 1` this is a latest-wins single slot.
struct FrameMailbox {
    /// Maximum number of buffered frames; at least 1.
    capacity: usize,
    state: Mutex<MailboxState>,
    available: Condvar,
}

struct MailboxState {
    /// Buffered frames, oldest at the front. Capped at `FrameMailbox::capacity`.
    frames: std::collections::VecDeque<Frame>,
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
                closed: false,
            }),
            available: Condvar::new(),
        }
    }

    /// Producer: append `frame` to the buffer. When the buffer is full the oldest
    /// frame is dropped (and freed) to make room -- newest always wins. A no-op
    /// once closed.
    fn push(&self, frame: Frame) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.closed {
            return; // `frame` is dropped here -> its buffer is freed.
        }
        if state.frames.len() == self.capacity {
            state.frames.pop_front(); // drop the oldest unconsumed frame -> freed.
        }
        state.frames.push_back(frame);
        drop(state);
        self.available.notify_one();
    }

    /// Consumer: block until a frame is buffered or the mailbox is closed. Returns
    /// the oldest buffered frame, or `None` once closed with nothing left to drain.
    fn recv(&self) -> Option<Frame> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            if let Some(frame) = state.frames.pop_front() {
                return Some(frame);
            }
            if state.closed {
                return None;
            }
            state = self.available.wait(state).unwrap_or_else(|p| p.into_inner());
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

/// State the OBS render callback needs to capture a frame and hand it off: the
/// reusable capture context (owns its GPU surfaces), the source to capture, the
/// calibrated region shared with the worker, and the mailbox to push into. Boxed
/// and passed to OBS as the callback `param`. Owns the capture context and
/// destroys it on drop (in `handle_stop`, after the callback is unregistered).
struct ProducerCtx {
    ctx: *mut crate::ffi::GeCaptureCtx,
    name: CString,
    region: Arc<Mutex<Option<CaptureRegion>>>,
    mailbox: Arc<FrameMailbox>,
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
    let len = (width * height * 4) as usize;
    producer.mailbox.push(Frame { buf: FrameBuf::CMalloc { ptr: frame, len }, width, height });
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartParams {
    /// Name of the OBS source to monitor, as reported by `/api/v1/sources`.
    source_name: String,
    /// Language of the templates to match against (e.g. `en`, `jp`).
    lang: String,
}

/// Source of frames for the monitor loop. OBS captures in production; tests
/// drive the same loop from decoded fixture images. The frame bytes are only
/// borrowed for the duration of `use_frame`, so the source can free or reuse the
/// backing buffer immediately afterwards (the OBS source frees its C buffer).
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
/// callback (`ge_frame_callback`) pushes into the shared mailbox. The capture
/// itself, and the capture context's GPU surfaces, live on the producer side;
/// this only blocks for the next frame and matches it.
struct ObsSource {
    mailbox: Arc<FrameMailbox>,
    /// The calibrated capture transform, shared with the producer callback.
    /// Latched on first sight: a stretched source's transform is fixed for the
    /// session, and once frames arrive pre-normalized the matcher reports no
    /// further calibration, so re-reading it would (incorrectly) clear it.
    region: Arc<Mutex<Option<CaptureRegion>>>,
}

impl FrameSource for ObsSource {
    fn capture<F, R>(&mut self, use_frame: F) -> Option<R>
    where
        F: FnOnce(&[u8], u32, u32) -> R,
    {
        // Block until the producer pushes the next frame, or the mailbox is
        // closed on stop (returns `None`, ending the run loop). The frame is
        // dropped at the end of this scope, freeing its C buffer.
        let frame = self.mailbox.recv()?;
        Some(use_frame(frame.buf.as_slice(), frame.width, frame.height))
    }

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
}

/// A monitor session: owns the matcher (and therefore its per-resolution scale
/// cache) for the lifetime of one start/stop cycle. Because the cache lives in
/// the matcher, dropping the session clears it -- so each `start` begins with a
/// cold cache and a source/resolution change is never matched against a stale
/// scale. Within a session, the cache keys on the source dimensions, so a
/// mid-session resolution change re-learns the scale on the next frame.
pub struct MonitorSession {
    matcher: CvMatcher,
}

impl MonitorSession {
    /// Builds a session with the given language, reading `GE_CV_TEMPLATE_DIR`
    /// from the environment (as the rest of the plugin does).
    pub fn from_env(lang: &str) -> anyhow::Result<Self> {
        let template_dir =
            std::env::var("GE_CV_TEMPLATE_DIR").map_err(|_| anyhow::anyhow!("GE_CV_TEMPLATE_DIR is not set"))?;
        Self::new(lang, &template_dir)
    }

    /// Builds a session with an explicit language and template directory.
    pub fn new(lang: &str, template_dir: &str) -> anyhow::Result<Self> {
        let matcher = CvMatcher::new(lang, template_dir)
            .map_err(|err| anyhow::anyhow!("failed to init matcher: {}", err.message))?;
        Ok(MonitorSession { matcher })
    }

    /// Matches one BGRA frame. The matcher's scale cache makes the first overlay
    /// frame at a given resolution costlier (it searches for the scale) and every
    /// later frame at that resolution cheap (it reuses the learned scale).
    pub fn match_frame(&self, bytes: &[u8], width: u32, height: u32) -> opencv::Result<LevelMatch> {
        self.matcher.match_level_from_bgra_bytes(bytes, width, height)
    }

    /// Hot loop: take each frame `source` yields, match it, and pass the result
    /// to `on_result`. The source blocks until a frame is available, so there is
    /// no polling; it returns `None` when exhausted (test fixtures) or closed
    /// (the OBS mailbox on stop), which ends the loop.
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

/// Current monitor status. `enabled` is always present; the source/language are
/// only included while a monitor is running (omitted from the JSON otherwise).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorStatus {
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lang: Option<String>,
}

/// Reports whether a monitor is currently running, and if so which source and
/// language it was started with -- so the frontend can restore its state on load.
#[axum::debug_handler]
pub async fn handle_status(State(state): State<AppState>) -> Json<MonitorStatus> {
    let guard = state.monitor.lock().unwrap_or_else(|p| p.into_inner());
    let status = match guard.as_ref() {
        Some(handle) => MonitorStatus {
            enabled: true,
            source_name: Some(handle.source_name.clone()),
            lang: Some(handle.lang.clone()),
        },
        None => MonitorStatus { enabled: false, source_name: None, lang: None },
    };
    Json(status)
}

#[axum::debug_handler]
pub async fn handle_start(State(state): State<AppState>, Json(params): Json<StartParams>) -> Result<impl IntoResponse> {
    // Keep the original strings for the status endpoint; `source_name` is also
    // converted to a CString below for the C capture bridge.
    let status_source_name = params.source_name.clone();
    let lang = params.lang.clone();
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

    // Build the session (and its fresh, empty scale cache) up front so any
    // configuration error surfaces as a failed request rather than a thread that
    // silently exits.
    let session = MonitorSession::from_env(&params.lang).map_err(|err| {
        tracing::error!("failed to start monitor: {err}");
        (StatusCode::INTERNAL_SERVER_ERROR, "failed to init matcher")
    })?;

    // Reusable capture context (and its GPU surfaces), created once for the
    // session. Owned by the ProducerCtx below and destroyed when that is dropped
    // on stop. Double-buffered: the render callback runs on the graphics thread,
    // so pipelining the readback (map last frame while staging this one) keeps it
    // from stalling OBS's render. The first frame after start (or a resolution
    // change) only primes the pipeline and yields no frame -- the callback's
    // null check skips it.
    let ctx = unsafe { crate::ffi::ge_capture_create(true) };
    if ctx.is_null() {
        tracing::error!("failed to create capture context; monitor not started");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "failed to create capture context").into());
    }

    // Shared between the OBS producer (render callback) and the worker consumer:
    // the frame mailbox and the latched capture region. Capacity 1 keeps only the
    // freshest frame (drop-oldest), so the matcher never lags behind real time;
    // raise it to retain a short backlog.
    let mailbox = Arc::new(FrameMailbox::new(FRAME_BUFFER_CAPACITY));
    let region = Arc::new(Mutex::new(None));

    // Producer state handed to OBS as the render-callback param. Boxed and leaked
    // to a raw pointer for the monitor's lifetime; reclaimed (and the capture
    // context destroyed) in `handle_stop`.
    let producer = Box::into_raw(Box::new(ProducerCtx {
        ctx,
        name: source_name,
        region: region.clone(),
        mailbox: mailbox.clone(),
    }));

    // From here on OBS pushes a captured frame into the mailbox once per rendered
    // frame -- the push model that replaces the old capture-in-a-spin-loop.
    unsafe { crate::ffi::ge_obs_register_frame_callback(ge_frame_callback, producer.cast()) };

    // Run the matcher on a dedicated OS thread so its blocking, CPU-bound work
    // never ties up the async runtime's worker threads. The session is moved
    // onto the thread and dropped when the loop exits, clearing the cache.
    let worker_mailbox = mailbox.clone();
    // Broadcast each new match to connected WebSocket clients. We dedup here so
    // the channel only fires when the matched state actually changes (ignoring
    // `runtime_ms`), rather than every frame.
    let match_tx = state.match_tx.clone();
    // Handed to the recorder so it can broadcast a `RecordingSaved` event once a
    // run's clip is written out of the replay buffer.
    let event_tx = state.event_tx.clone();
    let thread = std::thread::Builder::new().name("ge-monitor".to_owned()).spawn(move || {
        let mut source = ObsSource { mailbox: worker_mailbox, region };
        let mut last: Option<LevelMatch> = None;
        // Drives the replay-buffer save/trim as the session progresses. Fed
        // every matched frame (not just state changes) so its save timer is
        // polled each tick.
        let mut recording = crate::recording::RecordingState::new(event_tx, recording_options);
        session.run(&mut source, |result| match result {
            Ok(info) => {
                tracing::debug!(?info);
                recording.on_frame(std::time::Instant::now(), &info);
                let changed = last.as_ref().is_none_or(|prev| !prev.same_state(&info));
                if changed {
                    tracing::info!(?info);
                    last = Some(info.clone());
                    // Ignore send errors: with no subscribers there is no
                    // receiver, but `watch` still retains the value for the
                    // next client to connect.
                    let _ = match_tx.send(Some(info));
                }
            }
            Err(e) => tracing::error!("err: {}", e.message),
        });
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

    *guard =
        Some(MonitorHandle { mailbox, producer: ProducerPtr(producer), thread, source_name: status_source_name, lang });
    tracing::info!("monitor started");

    Ok(StatusCode::OK)
}

#[axum::debug_handler]
pub async fn handle_stop(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let handle = {
        let mut guard = state.monitor.lock().unwrap_or_else(|p| p.into_inner());
        guard.take()
    };

    let Some(handle) = handle else {
        return Err((StatusCode::CONFLICT, "no monitor is running").into());
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
        // Stop new frames first. `ge_obs_unregister_frame_callback` serializes
        // with callback invocation, so once it returns the producer callback is
        // neither running nor will run again -- the ProducerCtx (and its capture
        // context) is then safe to free below.
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

    // Clear the last broadcast match so WebSocket clients see the monitor has
    // stopped (and a later `start` doesn't briefly replay the previous run's
    // final match before a fresh one is matched).
    let _ = state.match_tx.send(None);

    tracing::info!("monitor stopped");

    Ok(StatusCode::OK)
}

/// Upgrades the connection to a WebSocket that streams [`MonitorEvent`]s as JSON.
/// The current match (if any) is sent immediately on connect as a `match` event,
/// then a new `match` is pushed each time the matched state changes (see
/// [`AppStateInner::match_tx`](crate::http::AppStateInner)); one-off events such
/// as `recordingSaved` are forwarded as they occur (see
/// [`AppStateInner::event_tx`](crate::http::AppStateInner)).
pub async fn handle_ws(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.match_tx.subscribe();
    let mut events = state.event_tx.subscribe();

    // Announce which build serves this API first, so a stale tab (older cached
    // page, or one open across a plugin update) can compare it against its own
    // build and reload before it starts acting on match/recording events.
    let version = MonitorEvent::Version { build_id: super::index::BUILD_ID.clone() };
    if send_event(&mut socket, &version).await.is_err() {
        return;
    }

    // Send the current match up front so a client connecting mid-run isn't
    // blank until the next change.
    if send_current_match(&mut socket, &mut rx).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            // The match state changed: forward it as a `match` event.
            changed = rx.changed() => {
                // Err means the sender was dropped (server shutting down).
                if changed.is_err() {
                    break;
                }
                if send_current_match(&mut socket, &mut rx).await.is_err() {
                    break;
                }
            }
            // A one-off event was broadcast: forward it verbatim.
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        if send_event(&mut socket, &event).await.is_err() {
                            break;
                        }
                    }
                    // This client lagged and the channel dropped some events for
                    // it; nothing to forward for the skipped ones, so carry on.
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    // Sender dropped (server shutting down).
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // Drain inbound frames so we notice the client closing/erroring.
            // We don't expect any meaningful client messages.
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(_)) => {}
                    // Closed or errored.
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

/// Sends the current match (if any) as a `match` event, marking the watch value
/// as seen. A `None` value (no monitor running) sends nothing.
async fn send_current_match(
    socket: &mut WebSocket,
    rx: &mut watch::Receiver<Option<LevelMatch>>,
) -> Result<(), axum::Error> {
    let current = rx.borrow_and_update().clone();
    if let Some(m) = current {
        send_event(socket, &MonitorEvent::Match(m)).await?;
    }
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
        Frame { buf: FrameBuf::Owned(vec![tag]), width, height: 1 }
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
