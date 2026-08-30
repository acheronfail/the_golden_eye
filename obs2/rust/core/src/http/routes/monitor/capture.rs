use std::ffi::CString;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use crate::cv::CaptureRegion;

/// A running monitor. OBS pushes captured frames into `mailbox` (keyed by the
/// registered `producer`); the worker `thread` matches them. Stopping drops the
/// registration, closes the mailbox, and joins the worker.
pub struct MonitorHandle {
    pub(super) mailbox: Arc<FrameMailbox>,
    pub(super) producer: crate::obs::RegisteredRenderCallback<ProducerCtx>,
    pub(super) thread: JoinHandle<()>,
    /// The source name this monitor uses, retained in the shared app snapshot.
    pub(super) source_name: String,
    /// Durable catalog session for this monitor lifecycle, when creation succeeded.
    pub(super) session_id: Option<String>,
    /// The latched capture transform, shared so a standalone frame dump on the
    /// same source can crop/un-stretch its frames identically to the matcher.
    pub(super) region: Arc<Mutex<Option<CaptureRegion>>>,
    pub(super) recent_run_limit: Arc<AtomicUsize>,
}

impl MonitorHandle {
    pub(crate) fn set_recent_run_limit(&self, limit: usize) {
        self.recent_run_limit.store(limit.clamp(1, crate::recording::MAX_RECENT_RUN_LIMIT), Ordering::Release);
    }
}

/// A captured BGRA frame and its dimensions, owning its pixel buffer. Frames
/// from OBS wrap the C-`malloc`'d buffer the capture bridge returns; test frames
/// own a `Vec`.
pub(super) struct Frame {
    pub(super) buf: FrameBuf,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) captured_at: Option<Instant>,
    pub(super) capture_ms: Option<f64>,
    pub(super) callback_interval_ms: Option<f64>,
    pub(super) capture_timings: Option<crate::obs::GeCaptureTimings>,
    pub(super) dropped_frames_total: u64,
}

pub(super) enum FrameBuf {
    /// Buffer handed back by the safe OBS capture bridge.
    Obs(crate::obs::OwnedBgraFrame),
    /// Owned Rust buffer (test fixtures). Only constructed in tests; the OBS
    /// path always uses `Obs`.
    #[cfg_attr(not(test), allow(dead_code))]
    Owned(Vec<u8>),
}

impl FrameBuf {
    pub(super) fn as_slice(&self) -> &[u8] {
        match self {
            FrameBuf::Obs(frame) => frame.bytes(),
            FrameBuf::Owned(bytes) => bytes,
        }
    }
}

/// How many captured frames the mailbox buffers. 1 = always match the freshest
/// frame (drop any older unconsumed one); a larger value retains a short backlog.
pub(super) const FRAME_BUFFER_CAPACITY: usize = 1;

/// A bounded, drop-oldest FIFO frame buffer between the OBS producer and the
/// monitor consumer. Holds up to `capacity` frames; when full, the oldest is
/// dropped/freed so the matcher never falls behind. `capacity == 1` is latest-wins.
pub(super) struct FrameMailbox {
    /// Maximum number of buffered frames; at least 1.
    capacity: usize,
    state: Mutex<MailboxState>,
    available: Condvar,
}

/// Outcome of a [`FrameMailbox::recv_until`] wait.
pub(super) enum MailboxRecv {
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
    pub(super) fn new(capacity: usize) -> Self {
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
    pub(super) fn push(&self, mut frame: Frame) {
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
    pub(super) fn recv_until(&self, deadline: Option<Instant>) -> MailboxRecv {
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
    pub(super) fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.closed = true;
        drop(state);
        self.available.notify_one();
    }
}

/// State the OBS render callback needs to capture a frame and hand it off:
/// capture context, source name, calibrated region, and mailbox. Boxed as the
/// callback `param`; owns the capture context and destroys it on drop.
pub(super) struct ProducerCtx {
    pub(super) ctx: crate::obs::CaptureContext,
    pub(super) name: CString,
    pub(super) region: Arc<Mutex<Option<CaptureRegion>>>,
    pub(super) mailbox: Arc<FrameMailbox>,
    pub(super) timing_enabled: bool,
    pub(super) last_callback_at: Mutex<Option<Instant>>,
}

fn callback_interval_ms(
    enabled: bool,
    last_callback_at: &Mutex<Option<Instant>>,
    now: impl FnOnce() -> Instant,
) -> Option<f64> {
    if !enabled {
        return None;
    }
    let now = now();
    let mut last = last_callback_at.lock().unwrap_or_else(|p| p.into_inner());
    let interval = last.map(|previous| now.duration_since(previous).as_secs_f64() * 1000.0);
    *last = Some(now);
    interval
}

/// OBS render callback: capture one frame of the monitored source and push it
/// into the mailbox. Runs on the graphics thread inside a graphics context, once
/// per rendered frame.
impl crate::obs::RenderCallback for ProducerCtx {
    fn render_frame(&mut self, _canvas_width: u32, _canvas_height: u32) {
        let callback_interval_ms = callback_interval_ms(self.timing_enabled, &self.last_callback_at, Instant::now);

        let region = {
            let guard = self.region.lock().unwrap_or_else(|p| p.into_inner());
            guard.map(|r| {
                let out_height = crate::cv::WORK_HEIGHT as u32;
                let out_width = ((out_height as f32 * r.out_aspect).round() as u32).max(1);
                crate::obs::GeCaptureRegion {
                    crop_x: r.crop_x,
                    crop_y: r.crop_y,
                    crop_w: r.crop_w,
                    crop_h: r.crop_h,
                    out_width,
                    out_height,
                }
            })
        };
        let max_height = if region.is_some() { 0 } else { crate::cv::WORK_HEIGHT as u32 };
        let mut capture_timings = self.timing_enabled.then(crate::obs::GeCaptureTimings::default);
        let capture_started = self.timing_enabled.then(Instant::now);
        let Some(frame) = self.ctx.capture(&self.name, max_height, region.as_ref(), capture_timings.as_mut()) else {
            return;
        };
        let (captured_at, capture_ms) = if let Some(capture_started) = capture_started {
            let captured_at = Instant::now();
            (Some(captured_at), Some(captured_at.duration_since(capture_started).as_secs_f64() * 1000.0))
        } else {
            (None, None)
        };
        let width = frame.width();
        let height = frame.height();
        self.mailbox.push(Frame {
            buf: FrameBuf::Obs(frame),
            width,
            height,
            captured_at,
            capture_ms,
            callback_interval_ms,
            capture_timings,
            dropped_frames_total: 0,
        });
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CapturedFrameStats {
    pub(super) capture_ms: Option<f64>,
    pub(super) callback_interval_ms: Option<f64>,
    pub(super) capture_timings: Option<crate::obs::GeCaptureTimings>,
    pub(super) mailbox_wait_ms: Option<f64>,
    pub(super) dropped_frames_total: u64,
}

#[cfg(test)]
#[path = "capture_test.rs"]
mod capture_test;
