//! Monitor lifecycle and coordination, independent of HTTP request handling.

use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::SystemTime;

use tokio::sync::broadcast;

use super::matcher::MonitorMatcher;
use super::publication::MonitorStoppedReason;
use super::session::RunSession;
use super::worker::FrameWorker;
use super::{
    DEFAULT_MONITOR_LANGUAGE,
    MAX_RECENT_RUN_LIMIT,
    RecordingSessionContext,
    RecordingStateEvent,
    RecordingStateStore,
    RunRecorder,
};
use crate::app::{AppEvent, SharedStateStore};
use crate::config::MonitorTimingMode;
use crate::cv::CaptureRegion;
use crate::db::run_catalog::RunCatalog;
use crate::obs::frame_capture::{FRAME_BUFFER_CAPACITY, FrameMailbox, ObsSource, ProducerCtx};
use crate::run_monitoring::publication::ReplaySaveStateStore;
use crate::settings::SettingsStore;

/// Owns monitor resources and lifecycle operations. The worker owns per-frame state.
pub(crate) struct RunMonitor {
    active: Mutex<Option<MonitorHandle>>,
    stop_after_replay: AtomicBool,
    annotations_enabled: Arc<AtomicBool>,
    settings: Arc<SettingsStore>,
    run_catalog: Arc<RunCatalog>,
    snapshot: SharedStateStore,
    event_tx: broadcast::Sender<AppEvent>,
    recording_state: RecordingStateStore,
    replay_saves: ReplaySaveStateStore,
}

#[derive(Debug)]
pub(crate) enum StartError {
    InvalidSourceName,
    AlreadyRunning,
    ReplayBufferUnavailable,
    MatcherUnavailable,
    CaptureUnavailable,
    WorkerUnavailable,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum StopReason {
    UserStopped,
    ReplayBufferStopped,
    CoreReload,
    ObsShutdown,
}

impl StopReason {
    fn catalog_reason(self) -> &'static str {
        match self {
            Self::UserStopped => "userStopped",
            Self::ReplayBufferStopped => "replayBufferStopped",
            Self::CoreReload => "coreReload",
            Self::ObsShutdown => "obsShutdown",
        }
    }
}

impl RunMonitor {
    pub(crate) fn new(
        settings: Arc<SettingsStore>,
        run_catalog: Arc<RunCatalog>,
        snapshot: SharedStateStore,
        event_tx: broadcast::Sender<AppEvent>,
        recording_state: RecordingStateStore,
        replay_saves: ReplaySaveStateStore,
    ) -> Self {
        Self {
            active: Mutex::new(None),
            stop_after_replay: AtomicBool::new(false),
            annotations_enabled: Arc::new(AtomicBool::new(false)),
            settings,
            run_catalog,
            snapshot,
            event_tx,
            recording_state,
            replay_saves,
        }
    }

    pub(crate) fn start(&self, status_source_name: String) -> Result<(), StartError> {
        let recording_options = self.settings.get_recording_options();
        let recent_run_limit =
            Arc::new(AtomicUsize::new(recording_options.recent_run_limit.clamp(1, MAX_RECENT_RUN_LIMIT)));
        let source_name = CString::new(status_source_name.clone()).map_err(|_| StartError::InvalidSourceName)?;

        // Starting the current source is idempotent so a reconnecting frontend can
        // safely converge on backend state. A different source remains a conflict.
        let mut guard = self.active.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(handle) = guard.as_ref() {
            return if handle.source_name == status_source_name { Ok(()) } else { Err(StartError::AlreadyRunning) };
        }

        if !crate::run_monitoring::replay_buffer::REPLAY_BUFFER.ensure_replay_buffer_running() {
            return Err(StartError::ReplayBufferUnavailable);
        }
        self.recording_state.handle(RecordingStateEvent::Reset);

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
        let run_catalog = self.run_catalog.clone();
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
        let event_tx = self.event_tx.clone();
        let recording_state = self.recording_state.clone();
        let replay_saves = self.replay_saves.clone();
        let snapshot = self.snapshot.clone();
        let annotations_enabled = self.annotations_enabled.clone();
        let worker_source_name = status_source_name.clone();
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
        let worker_recent_run_limit = recent_run_limit.clone();
        let recording_context = RecordingSessionContext::new(
            status_source_name.clone(),
            DEFAULT_MONITOR_LANGUAGE.to_owned(),
            monitor_session_id.clone(),
        );
        let thread = std::thread::Builder::new().name("ge-monitor".to_owned()).spawn(move || {
            let mut recording = RunRecorder::new(
                event_tx.clone(),
                recording_state,
                replay_saves,
                recording_options,
                recording_context,
                run_catalog,
            );
            recording.set_recent_run_limit_source(worker_recent_run_limit);
            let mut run_session = RunSession::new(snapshot, recording);
            let worker =
                FrameWorker::new(session, event_tx.clone(), annotations_enabled, source_fps, monitor_timing_mode);
            run_session.start(worker_source_name, DEFAULT_MONITOR_LANGUAGE.to_owned());
            let _ = started_tx.send(());
            worker.run(ObsSource { mailbox: worker_mailbox, region }, &mut run_session);
            // Flush pending saves before measuring the final session/level time.
            run_session.stop();
        });
        let thread = match thread {
            Ok(thread) => thread,
            Err(err) => {
                tracing::error!("failed to spawn monitor thread: {err}");
                drop(producer);
                if let Some(session_id) = monitor_session_id.as_deref()
                    && let Err(error) = self.run_catalog.delete_empty_monitor_session(session_id)
                {
                    tracing::warn!("failed to remove provisional monitoring session: {error:#}");
                }
                return Err(StartError::WorkerUnavailable);
            }
        };

        // A successful start response guarantees publication, even with no captured frames.
        if started_rx.recv().is_err() {
            drop(producer);
            mailbox.close();
            let _ = thread.join();
            if let Some(session_id) = monitor_session_id.as_deref() {
                let _ = self.run_catalog.delete_empty_monitor_session(session_id);
            }
            return Err(StartError::WorkerUnavailable);
        }

        *guard = Some(MonitorHandle {
            mailbox,
            producer,
            thread,
            source_name: status_source_name.clone(),
            session_id: monitor_session_id,
            region: handle_region,
            recent_run_limit,
        });
        self.snapshot.set_replay_buffer(crate::obs::current_replay_buffer_status());
        tracing::info!("monitor started");

        Ok(())
    }

    /// Stop the active monitor, if any, and clear all retained monitor/recording
    /// state. Returns `false` when no monitor was running.
    pub(crate) async fn stop(&self, reason: StopReason) -> bool {
        let handle = {
            let mut guard = self.active.lock().unwrap_or_else(|p| p.into_inner());
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
            && let Err(err) =
                self.run_catalog.end_monitor_session(session_id, Some(session_ended_at), reason.catalog_reason())
        {
            tracing::warn!("failed to close monitoring session {session_id}: {err:#}");
        }

        // The worker has published its final monitor snapshot; clear the retained recording phase.
        self.recording_state.handle(RecordingStateEvent::Reset);

        if self.settings.get().stop_replay_buffer_when_monitor_stopped {
            crate::run_monitoring::replay_buffer::REPLAY_BUFFER.stop_replay_buffer_if_active();
            self.snapshot.set_replay_buffer(crate::obs::current_replay_buffer_status());
        }

        tracing::info!("monitor stopped");
        let notification = match reason {
            StopReason::UserStopped => Some(MonitorStoppedReason::UserStopped),
            StopReason::ReplayBufferStopped => {
                tracing::warn!("replay buffer stopped while monitoring was active; monitoring disabled");
                Some(MonitorStoppedReason::ReplayBufferStopped)
            }
            StopReason::CoreReload | StopReason::ObsShutdown => None,
        };
        if let Some(reason) = notification {
            let _ = self.event_tx.send(AppEvent::MonitorStopped { reason });
        }
        true
    }

    /// Capture intent before OBS stops: intentional monitor shutdown has already taken the handle.
    pub(crate) fn replay_buffer_stopping(&self) {
        self.stop_after_replay.store(self.is_active(), Ordering::Release);
    }

    /// Consume on the synchronous OBS callback before scheduling asynchronous teardown.
    pub(crate) fn take_replay_stop_request(&self) -> bool {
        self.stop_after_replay.swap(false, Ordering::AcqRel)
    }

    /// Whether a session handle is held; becomes false when shutdown takes it.
    /// This does not wait for the departing worker or pending saves to finish.
    pub(crate) fn is_active(&self) -> bool {
        self.active.lock().unwrap_or_else(|p| p.into_inner()).is_some()
    }

    pub(crate) fn set_recent_run_limit(&self, limit: usize) {
        if let Some(handle) = self.active.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            handle.recent_run_limit.store(limit.clamp(1, MAX_RECENT_RUN_LIMIT), Ordering::Release);
        }
    }

    pub(crate) fn set_annotations_enabled(&self, enabled: bool) {
        self.annotations_enabled.store(enabled, Ordering::Release);
    }

    /// Share the live transform with a diagnostic frame dump of the same source.
    pub(crate) fn capture_region(&self, source_name: &str) -> Option<Arc<Mutex<Option<CaptureRegion>>>> {
        self.active
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .filter(|handle| handle.source_name == source_name)
            .map(|handle| handle.region.clone())
    }
}

/// A running monitor. OBS pushes captured frames into `mailbox` (keyed by the
/// registered `producer`); the worker `thread` matches them. Stopping drops the
/// registration, closes the mailbox, and joins the worker.
struct MonitorHandle {
    mailbox: Arc<FrameMailbox>,
    producer: crate::obs::RegisteredRenderCallback<ProducerCtx>,
    thread: JoinHandle<()>,
    /// The source name this monitor uses, retained in the shared app snapshot.
    source_name: String,
    /// Durable catalog session for this monitor lifecycle, when creation succeeded.
    session_id: Option<String>,
    /// The latched capture transform, shared so a standalone frame dump on the
    /// same source can crop/un-stretch its frames identically to the matcher.
    region: Arc<Mutex<Option<CaptureRegion>>>,
    recent_run_limit: Arc<AtomicUsize>,
}
