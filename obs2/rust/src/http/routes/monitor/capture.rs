use std::ffi::{CString, c_void};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use crate::cv::CaptureRegion;

/// A running monitor. OBS pushes captured frames into `mailbox` (keyed by the
/// leaked `producer`); the worker `thread` matches them. Stopping unregisters
/// the callback, closes the mailbox to wake+join the worker, then frees producer.
pub struct MonitorHandle {
    pub(super) mailbox: Arc<FrameMailbox>,
    pub(super) producer: ProducerPtr,
    pub(super) thread: JoinHandle<()>,
    /// The source name this monitor uses, retained in the shared app snapshot.
    pub(super) source_name: String,
    /// The latched capture transform, shared so a standalone frame dump on the
    /// same source can crop/un-stretch its frames identically to the matcher.
    pub(super) region: Arc<Mutex<Option<CaptureRegion>>>,
}

/// The leaked `ProducerCtx` pointer, made `Send` so the handle can move to the
/// blocking teardown task. SAFETY: only dereferenced on the OBS graphics thread;
/// freed only after `ge_obs_unregister_frame_callback` ensures no callback runs.
pub(super) struct ProducerPtr(pub(super) *mut ProducerCtx);
unsafe impl Send for ProducerPtr {}

/// A captured BGRA frame and its dimensions, owning its pixel buffer. Frames
/// from OBS wrap the C-`malloc`'d buffer the capture bridge returns; test frames
/// own a `Vec`.
pub(super) struct Frame {
    pub(super) buf: FrameBuf,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) captured_at: Option<Instant>,
    pub(super) capture_ms: Option<f64>,
    pub(super) dropped_frames_total: u64,
}

// SAFETY: a `Frame` owns its buffer exclusively and never aliases the raw
// pointer once constructed, so moving it from the producer (graphics) thread to
// the consumer (monitor) thread through the mailbox is sound.
unsafe impl Send for Frame {}

pub(super) enum FrameBuf {
    /// Buffer handed back by `ge_capture_get_frame`; released with the C `free`.
    CMalloc { ptr: *mut u8, len: usize },
    /// Owned Rust buffer (test fixtures). Only constructed in tests; the OBS
    /// path always uses `CMalloc`.
    #[cfg_attr(not(test), allow(dead_code))]
    Owned(Vec<u8>),
}

impl FrameBuf {
    pub(super) fn as_slice(&self) -> &[u8] {
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
    pub(super) ctx: *mut crate::ffi::GeCaptureCtx,
    pub(super) name: CString,
    pub(super) region: Arc<Mutex<Option<CaptureRegion>>>,
    pub(super) mailbox: Arc<FrameMailbox>,
    pub(super) timing_enabled: bool,
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
pub(super) unsafe extern "C" fn ge_frame_callback(param: *mut c_void, _cx: u32, _cy: u32) {
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
pub(super) struct CapturedFrameStats {
    pub(super) capture_ms: f64,
    pub(super) mailbox_wait_ms: f64,
    pub(super) dropped_frames_total: u64,
}

#[cfg(test)]
#[path = "capture_test.rs"]
mod capture_test;
