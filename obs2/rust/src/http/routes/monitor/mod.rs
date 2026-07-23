use std::ffi::CString;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::Deserialize;

use crate::cv::{CaptureRegion, LevelMatch};
use crate::http::{AppEvent, AppState, MonitorFps, MonitorStoppedReason};

mod capture;
mod frame_dump;
mod session;

pub use capture::MonitorHandle;
use capture::{
    CapturedFrameStats,
    FRAME_BUFFER_CAPACITY,
    FrameMailbox,
    MailboxRecv,
    ProducerCtx,
    ProducerPtr,
    ge_frame_callback,
};
pub use session::MonitorSession;
use session::{DisplayTimeSmoother, handle_detected_language, log_level_match};

const DEFAULT_MONITOR_LANGUAGE: &str = "jp";
const MONITOR_FPS_EMIT_INTERVAL: Duration = Duration::from_millis(100);
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartParams {
    /// Name of the OBS source to monitor, as reported by `/api/v1/sources`.
    source_name: String,
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

use crate::config::MonitorTimingMode;

struct MonitorTiming {
    mode: MonitorTimingMode,
    slow_ms: f64,
    last_dropped_frames_total: u64,
}

impl MonitorTiming {
    fn new(source_fps: f64, mode: MonitorTimingMode) -> Self {
        let slow_ms = crate::config::default_monitor_slow_ms(source_fps);

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

#[axum::debug_handler]
pub async fn handle_start(State(state): State<AppState>, Json(params): Json<StartParams>) -> Result<impl IntoResponse> {
    // Keep the original source name for the app snapshot; it is also converted
    // to a CString below for the C capture bridge.
    let status_source_name = params.source_name.clone();
    let effective_settings = state.settings.get_effective();
    let catalog_state = state.clone();
    tokio::task::spawn_blocking(move || {
        super::runs::seed_catalog_if_needed(&catalog_state, &effective_settings);
        if let Err(err) = catalog_state.run_catalog.cleanup_recent(effective_settings.recent_run_limit) {
            tracing::warn!("failed to clean recent-run history before monitor start: {err:#}");
        }
    })
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "run catalog task failed"))?;
    let recording_options = state.settings.get_recording_options();
    let source_name =
        CString::new(params.source_name).map_err(|_| (StatusCode::BAD_REQUEST, "source name contains a null byte"))?;

    // Starting the current source is idempotent so a reconnecting frontend can
    // safely converge on backend state. A different source remains a conflict.
    let mut guard = state.monitor.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(handle) = guard.as_ref() {
        return if handle.source_name == status_source_name {
            Ok(StatusCode::OK)
        } else {
            Err((StatusCode::CONFLICT, "a monitor is already running").into())
        };
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
    let run_catalog = state.run_catalog.clone();
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
            run_catalog.clone(),
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
                    let _ = event_tx.send(AppEvent::MonitorFps(MonitorFps { processed_fps, source_fps }));
                }
                last_fps_emit = now;
                slowest_frame_fps = None;
            }

            // Once the matcher has calibrated this source's aspect, hand the
            // transform to the capture layer so subsequent frames are cropped +
            // un-stretched on the GPU at capture time.
            source.set_capture_region(session.capture_region());

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
    let _ = state.event_tx.send(AppEvent::MonitorStopped { reason: MonitorStoppedReason::UserStopped });

    Ok(StatusCode::OK)
}

pub use frame_dump::{FrameDumpHandle, handle_frame_dump};
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
