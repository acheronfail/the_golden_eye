use std::ffi::CString;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::Deserialize;

use crate::cv::{CaptureRegion, LevelMatch, WatchDetector, detect_black_frame, detect_watch};
use crate::http::{AppEvent, AppState, MonitorStoppedReason};

mod capture;
mod frame_dump;
mod session;
mod throughput;

pub use capture::MonitorHandle;
use capture::{CapturedFrameStats, FRAME_BUFFER_CAPACITY, FrameMailbox, MailboxRecv, ProducerCtx};
pub use session::MonitorSession;
use session::{DisplayTimeSmoother, log_level_match, switch_detected_language};
use throughput::ThroughputMeter;

const DEFAULT_MONITOR_LANGUAGE: &str = "jp";
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartParams {
    /// Name of the OBS source to monitor, as reported by `/api/v1/sources`.
    source_name: String,
}

/// Frame source backed by the live OBS source: consumes the frames the render
/// callback pushes into the shared mailbox. Capture and
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
        let stats = CapturedFrameStats {
            capture_ms: frame.capture_ms,
            callback_interval_ms: frame.callback_interval_ms,
            capture_timings: frame.capture_timings,
            mailbox_wait_ms: frame.captured_at.map(|at| at.elapsed().as_secs_f64() * 1000.0),
            dropped_frames_total: frame.dropped_frames_total,
        };
        let result = use_frame(frame.buf.as_slice(), frame.width, frame.height);
        Captured::Frame(result, stats)
    }
}

/// Outcome of [`ObsSource::capture_with_stats_until`].
enum Captured<R> {
    /// A frame was matched, with optional capture timing.
    Frame(R, CapturedFrameStats),
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
        stats: CapturedFrameStats,
        match_ms: Option<f64>,
        cv_runtime_ms: Option<f64>,
        source_fps: f64,
    ) {
        if self.mode == MonitorTimingMode::Off {
            return;
        }
        let (Some(capture_ms), Some(capture_timings), Some(mailbox_wait_ms), Some(match_ms)) =
            (stats.capture_ms, stats.capture_timings, stats.mailbox_wait_ms, match_ms)
        else {
            return;
        };

        let dropped_frames = stats.dropped_frames_total.saturating_sub(self.last_dropped_frames_total);
        self.last_dropped_frames_total = stats.dropped_frames_total;
        let total_ms = capture_ms + mailbox_wait_ms + match_ms;
        let slow = total_ms >= self.slow_ms || dropped_frames > 0;

        if slow {
            tracing::warn!(
                callback_interval_ms = stats.callback_interval_ms,
                capture_ms,
                capture_source_ms = capture_timings.source_ms,
                capture_allocation_ms = capture_timings.allocation_ms,
                capture_render_stage_ms = capture_timings.render_stage_ms,
                capture_map_copy_ms = capture_timings.map_copy_ms,
                capture_cleanup_ms = capture_timings.cleanup_ms,
                mailbox_wait_ms,
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
                callback_interval_ms = stats.callback_interval_ms,
                capture_ms,
                capture_source_ms = capture_timings.source_ms,
                capture_allocation_ms = capture_timings.allocation_ms,
                capture_render_stage_ms = capture_timings.render_stage_ms,
                capture_map_copy_ms = capture_timings.map_copy_ms,
                capture_cleanup_ms = capture_timings.cleanup_ms,
                mailbox_wait_ms,
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
    })
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "run catalog task failed"))?;
    let recording_options = state.settings.get_recording_options();
    let recent_run_limit = Arc::new(std::sync::atomic::AtomicUsize::new(
        recording_options.recent_run_limit.clamp(1, crate::recording::MAX_RECENT_RUN_LIMIT),
    ));
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
    let Some(ctx) = crate::ffi::CaptureContext::new(true) else {
        tracing::error!("failed to create capture context; monitor not started");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "failed to create capture context").into());
    };

    // Shared between the OBS producer (render callback) and the worker consumer:
    // the frame mailbox and latched capture region. Capacity 1 is drop-oldest
    // (freshest frame only); raise it to retain a short backlog.
    let mailbox = Arc::new(FrameMailbox::new(FRAME_BUFFER_CAPACITY));
    let region = Arc::new(Mutex::new(None));
    let monitor_timing_mode = MonitorTimingMode::from_env();

    let producer = crate::ffi::RegisteredRenderCallback::register(ProducerCtx {
        ctx,
        name: source_name,
        region: region.clone(),
        mailbox: mailbox.clone(),
        timing_enabled: monitor_timing_mode != MonitorTimingMode::Off,
        last_callback_at: Mutex::new(None),
    });

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
    let replay_saves = state.replay_saves.clone();
    let monitor_annotations_state = state.clone();
    let run_catalog = state.run_catalog.clone();
    let monitor_session_id = match run_catalog.create_monitor_session(
        SystemTime::now(),
        status_source_name.clone(),
        Some(DEFAULT_MONITOR_LANGUAGE.to_owned()),
        env!("GE_PLUGIN_VERSION").to_owned(),
    ) {
        Ok(session_id) => Some(session_id),
        Err(err) => {
            tracing::warn!("failed to persist monitoring session; continuing without association: {err:#}");
            None
        }
    };
    let recording_session_id = monitor_session_id.clone();
    let recording_source_name = status_source_name.clone();
    let recording_lang = DEFAULT_MONITOR_LANGUAGE.to_owned();
    let source_fps = crate::ffi::video_fps();
    // Kept for the handle so a standalone frame dump can share the latched region.
    let handle_region = region.clone();
    let worker_recent_run_limit = recent_run_limit.clone();
    let thread = std::thread::Builder::new().name("ge-monitor".to_owned()).spawn(move || {
        let mut source = ObsSource { mailbox: worker_mailbox, region };
        let mut session = session;
        let mut active_lang = recording_lang.clone();
        let mut last: Option<LevelMatch> = None;
        let mut display_smoother = DisplayTimeSmoother::new();
        let mut last_diagnostics_enabled = false;
        let mut throughput = ThroughputMeter::new(Instant::now(), source_fps);
        let mut monitor_timing = MonitorTiming::new(source_fps, monitor_timing_mode);
        let timing_enabled = monitor_timing.enabled();
        // Drives the replay-buffer save/trim as the session progresses. Fed
        // every matched frame (not just state changes) so its save timer is
        // polled each tick.
        let mut recording = crate::recording::RecordingState::new(
            event_tx.clone(),
            recording_state,
            replay_saves,
            recording_options,
            crate::recording::RecordingSessionContext::new(recording_source_name, recording_lang, recording_session_id),
            run_catalog.clone(),
        );
        recording.set_recent_run_limit_source(worker_recent_run_limit);
        let mut watch_detector = WatchDetector::default();
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
            let (result, black_frame, watch_signal, observed_at_unix_ms, match_ms, stats) = match source
                .capture_with_stats_until(deadline, |bytes, w, h| {
                    let observed_at_unix_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                        .try_into()
                        .unwrap_or(u64::MAX);
                    let match_started = timing_enabled.then(Instant::now);
                    let result = session.match_frame(bytes, w, h);
                    let active_picture = session.active_picture_region(w, h);
                    let black_frame = detect_black_frame(bytes, w, h, active_picture);
                    let watch_signal = detect_watch(bytes, w, h, active_picture);
                    let match_ms = match_started.map(|started| started.elapsed().as_secs_f64() * 1000.0);
                    (result, black_frame, watch_signal, observed_at_unix_ms, match_ms)
                }) {
                Captured::Frame((result, black_frame, watch_signal, observed_at_unix_ms, match_ms), stats) => {
                    (result, black_frame, watch_signal, observed_at_unix_ms, match_ms, stats)
                }
                Captured::Idle => {
                    recording.poll_pending(Instant::now());
                    continue;
                }
                Captured::Closed => break,
            };
            let now = Instant::now();
            if let Some(fps) = throughput.observe(now, stats.dropped_frames_total) {
                let _ = event_tx.send(AppEvent::MonitorFps(fps));
            }

            // Once the matcher has calibrated this source's aspect, hand the
            // transform to the capture layer so subsequent frames are cropped +
            // un-stretched on the GPU at capture time.
            source.set_capture_region(session.capture_region());

            match result {
                Ok(info) => {
                    monitor_timing.observe(stats, match_ms, Some(info.runtime_ms), source_fps);
                    tracing::debug!(?info);
                    if switch_detected_language(&info, &mut session, &mut active_lang, |lang| {
                        Ok(MonitorSession::from_env(lang)?.with_diagnostics(diagnostics_enabled))
                    }) {
                        snapshot.set_monitor_language(active_lang.clone());
                        recording.set_game_language(active_lang.clone());
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
            if let Some(signal) = black_frame {
                snapshot.observe_black_frame(signal);
            }
            if let Some(signal) = watch_signal
                && let Some(transition) = watch_detector.observe(signal).transition
            {
                snapshot.observe_watch_transition(transition, observed_at_unix_ms);
            }
        }
        tracing::info!("monitor loop exiting");
    });
    let thread = match thread {
        Ok(thread) => thread,
        Err(err) => {
            tracing::error!("failed to spawn monitor thread: {err}");
            drop(producer);
            if let Some(session_id) = monitor_session_id.as_deref()
                && let Err(error) = state.run_catalog.delete_empty_monitor_session(session_id)
            {
                tracing::warn!("failed to remove provisional monitoring session: {error:#}");
            }
            return Err((StatusCode::INTERNAL_SERVER_ERROR, "failed to spawn monitor thread").into());
        }
    };

    *guard = Some(MonitorHandle {
        mailbox,
        producer,
        thread,
        source_name: status_source_name.clone(),
        session_id: monitor_session_id,
        region: handle_region,
        recent_run_limit,
    });
    state.snapshot.set_monitor_running(status_source_name, DEFAULT_MONITOR_LANGUAGE.to_owned());
    state.snapshot.set_replay_buffer(crate::http::current_replay_buffer_status());
    tracing::info!("monitor started");

    Ok(StatusCode::OK)
}

#[axum::debug_handler]
pub async fn handle_stop(State(state): State<AppState>) -> Result<impl IntoResponse> {
    if !stop_monitor(&state, "userStopped").await {
        return Err((StatusCode::CONFLICT, "no monitor is running").into());
    }
    let _ = state.event_tx.send(AppEvent::MonitorStopped { reason: MonitorStoppedReason::UserStopped });

    Ok(StatusCode::OK)
}

pub use frame_dump::{FrameDumpHandle, handle_frame_dump};
/// Stop the active monitor, if any, and clear all retained monitor/recording
/// state. Returns `false` when no monitor was running.
pub(crate) async fn stop_monitor(state: &AppState, end_reason: &'static str) -> bool {
    let handle = {
        let mut guard = state.monitor.lock().unwrap_or_else(|p| p.into_inner());
        guard.take()
    };

    let Some(handle) = handle else {
        return false;
    };

    let session_id = handle.session_id.clone();
    let session_ended_at = SystemTime::now();

    // Tear down on a blocking thread so we don't stall the async runtime while
    // the in-flight match finishes. Joining the thread drops the session,
    // releasing the matcher and its scale cache.
    tokio::task::spawn_blocking(move || {
        let MonitorHandle { mailbox, producer, thread, .. } = handle;
        // Dropping the registration fences callbacks before reclaiming its state.
        drop(producer);
        // Wake the worker out of its blocking `recv` so the run loop exits.
        mailbox.close();
        if thread.join().is_err() {
            tracing::error!("monitor thread panicked");
        }
    })
    .await
    .ok();

    // Finalize only after the worker stops so its last frame cannot race an
    // empty-session delete while persisting the session's first run.
    if let Some(session_id) = session_id.as_deref()
        && let Err(err) = state.run_catalog.end_monitor_session(session_id, Some(session_ended_at), end_reason)
    {
        tracing::warn!("failed to close monitoring session {session_id}: {err:#}");
    }

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
