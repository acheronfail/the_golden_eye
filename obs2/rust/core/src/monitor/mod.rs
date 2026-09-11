//! Monitor lifecycle and coordination, independent of HTTP request handling.

use std::ffi::CString;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use crate::config::MonitorTimingMode;
use crate::http::AppState;

pub(crate) mod capture;
mod clocks;
mod matcher;
mod session;
mod throughput;
mod timing;

pub(crate) use capture::MonitorHandle;
use capture::{FRAME_BUFFER_CAPACITY, FrameMailbox, ObsSource, ProducerCtx};
use clocks::MonitorClockStore;
use matcher::MonitorMatcher;
use session::MonitorSession;

const DEFAULT_MONITOR_LANGUAGE: &str = "jp";

#[derive(Debug)]
pub(crate) enum StartError {
    InvalidSourceName,
    AlreadyRunning,
    ReplayBufferUnavailable,
    MatcherUnavailable,
    CaptureUnavailable,
    WorkerUnavailable,
}

pub(crate) fn start_monitor(state: &AppState, status_source_name: String) -> Result<(), StartError> {
    let recording_options = state.settings.get_recording_options();
    let recent_run_limit = Arc::new(std::sync::atomic::AtomicUsize::new(
        recording_options.recent_run_limit.clamp(1, crate::recording::MAX_RECENT_RUN_LIMIT),
    ));
    let source_name = CString::new(status_source_name.clone()).map_err(|_| StartError::InvalidSourceName)?;

    // Starting the current source is idempotent so a reconnecting frontend can
    // safely converge on backend state. A different source remains a conflict.
    let mut guard = state.monitor.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(handle) = guard.as_ref() {
        return if handle.source_name == status_source_name { Ok(()) } else { Err(StartError::AlreadyRunning) };
    }

    if !crate::recording::ensure_replay_buffer_running() {
        return Err(StartError::ReplayBufferUnavailable);
    }
    state.recording_state.clear();

    // Build the session (and its fresh, empty scale cache) up front so any
    // configuration error surfaces as a failed request rather than a thread that
    // silently exits.
    let session = MonitorMatcher::from_env(DEFAULT_MONITOR_LANGUAGE).map_err(|err| {
        tracing::error!("failed to start monitor: {err}");
        StartError::MatcherUnavailable
    })?;

    // Reusable capture context (and GPU surfaces), created once per session and
    // destroyed with the ProducerCtx on stop. Double-buffered so readback pipelines
    // without stalling OBS's render; the first frame only primes and yields none.
    let Some(ctx) = crate::obs::CaptureContext::new(true) else {
        tracing::error!("failed to create capture context; monitor not started");
        return Err(StartError::CaptureUnavailable);
    };

    // Shared between the OBS producer (render callback) and the worker consumer:
    // the frame mailbox and latched capture region. Capacity 1 is drop-oldest
    // (freshest frame only); raise it to retain a short backlog.
    let mailbox = Arc::new(FrameMailbox::new(FRAME_BUFFER_CAPACITY));
    let region = Arc::new(Mutex::new(None));
    let monitor_timing_mode = MonitorTimingMode::from_env();

    let producer = crate::obs::RegisteredRenderCallback::register(ProducerCtx {
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
    let source_fps = crate::obs::video_fps();
    let handle_region = region.clone();
    let worker_state = state.clone();
    let clocks = MonitorClockStore::new(state.snapshot.clone());
    let worker_clocks = clocks.clone();
    let worker_recent_run_limit = recent_run_limit.clone();
    let recording_context = crate::recording::RecordingSessionContext::new(
        status_source_name.clone(),
        DEFAULT_MONITOR_LANGUAGE.to_owned(),
        monitor_session_id.clone(),
    );
    let thread = std::thread::Builder::new().name("ge-monitor".to_owned()).spawn(move || {
        let mut recording = crate::recording::RecordingState::new(
            worker_state.event_tx.clone(),
            worker_state.recording_state.clone(),
            worker_state.replay_saves.clone(),
            recording_options,
            recording_context,
            run_catalog,
        );
        recording.set_recent_run_limit_source(worker_recent_run_limit);
        MonitorSession::new(session, recording, worker_state, source_fps, monitor_timing_mode, worker_clocks)
            .run(ObsSource { mailbox: worker_mailbox, region });
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
            return Err(StartError::WorkerUnavailable);
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
        clocks: clocks.clone(),
    });
    clocks.start_session(status_source_name, DEFAULT_MONITOR_LANGUAGE.to_owned());
    state.snapshot.set_replay_buffer(crate::http::current_replay_buffer_status());
    tracing::info!("monitor started");

    Ok(())
}

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
    let clocks = handle.clocks.clone();
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
    clocks.stop_session();
    state.recording_state.clear();

    if state.settings.get().stop_replay_buffer_when_monitor_stopped {
        crate::recording::stop_replay_buffer_if_active();
        state.snapshot.set_replay_buffer(crate::http::current_replay_buffer_status());
    }

    tracing::info!("monitor stopped");

    true
}
