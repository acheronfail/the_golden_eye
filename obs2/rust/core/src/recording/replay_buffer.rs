/// The latest replay-saved event, published by the OBS frontend callback and
/// awaited by the save thread.
struct ReplaySaved {
    /// Ticks per event so a waiter can tell a fresh event from a stale one.
    generation: u64,
    /// The file OBS just wrote, or `None` if it reported none.
    last_path: Option<String>,
    /// Plugin-initiated saves still awaiting their event; when zero, a saved
    /// event is the user's own manual save, which we leave untouched.
    pending_requests: u32,
}

static REPLAY_SAVED: Mutex<ReplaySaved> =
    Mutex::new(ReplaySaved { generation: 0, last_path: None, pending_requests: 0 });
static REPLAY_SAVED_CV: Condvar = Condvar::new();

/// Serializes plugin-initiated saves so at most one is outstanding: OBS's saved
/// event has no identity, so two in flight could both wake on it and trim the same
/// file. Only the request + wait need it (one at a time), not the subsequent trim.
#[cfg(not(test))]
static REPLAY_SAVE_SERIALIZE: Mutex<()> = Mutex::new(());

struct ReplayBufferLifecycle {
    starting: bool,
    stopping: bool,
    last_stopped_at: Option<Instant>,
}

static REPLAY_BUFFER_LIFECYCLE: Mutex<ReplayBufferLifecycle> =
    Mutex::new(ReplayBufferLifecycle { starting: false, stopping: false, last_stopped_at: None });
static REPLAY_BUFFER_LIFECYCLE_CV: Condvar = Condvar::new();
static REPLAY_BUFFER_ENSURE: Mutex<()> = Mutex::new(());

/// Publish a replay-saved event and wake any waiting save thread. Called (via
/// the `ge_replay_buffer_saved` FFI export) from the OBS frontend event
/// callback when `OBS_FRONTEND_EVENT_REPLAY_BUFFER_SAVED` fires.
pub fn on_replay_saved(path: Option<String>) {
    let mut guard = REPLAY_SAVED.lock().unwrap_or_else(|p| p.into_inner());
    // No plugin save is outstanding, so this is the user saving the buffer
    // themselves. Leave it alone: don't record it as ours, so no save thread
    // ever trims or deletes a file the user asked OBS to keep.
    if guard.pending_requests == 0 {
        tracing::debug!(?path, "ignoring user-initiated replay buffer save");
        return;
    }
    guard.pending_requests -= 1;
    guard.generation = guard.generation.wrapping_add(1);
    guard.last_path = path;
    drop(guard);
    REPLAY_SAVED_CV.notify_all();
}

/// Publish that OBS has begun starting the replay buffer.
pub fn on_replay_buffer_starting() {
    let mut guard = REPLAY_BUFFER_LIFECYCLE.lock().unwrap_or_else(|p| p.into_inner());
    if !guard.starting {
        tracing::debug!("replay buffer starting");
    }
    guard.starting = true;
    drop(guard);
    REPLAY_BUFFER_LIFECYCLE_CV.notify_all();
}

/// Publish that OBS has made the replay buffer active.
pub fn on_replay_buffer_started() {
    let mut guard = REPLAY_BUFFER_LIFECYCLE.lock().unwrap_or_else(|p| p.into_inner());
    if guard.starting {
        tracing::debug!("replay buffer started");
    }
    guard.starting = false;
    guard.last_stopped_at = None;
    drop(guard);
    REPLAY_BUFFER_LIFECYCLE_CV.notify_all();
}

/// Publish that OBS has begun stopping the replay buffer. This is also called
/// when we request a stop, because a quick monitor restart can reach
/// `/monitor/start` before OBS emits the frontend `STOPPING` event.
pub fn on_replay_buffer_stopping() {
    let mut guard = REPLAY_BUFFER_LIFECYCLE.lock().unwrap_or_else(|p| p.into_inner());
    if !guard.stopping {
        tracing::debug!("replay buffer stopping");
    }
    guard.starting = false;
    guard.stopping = true;
    guard.last_stopped_at = None;
    drop(guard);
    REPLAY_BUFFER_LIFECYCLE_CV.notify_all();
}

/// Publish that OBS has fully stopped the replay buffer and wake any monitor
/// start waiting to re-enable it.
pub fn on_replay_buffer_stopped() {
    let mut guard = REPLAY_BUFFER_LIFECYCLE.lock().unwrap_or_else(|p| p.into_inner());
    if guard.stopping {
        tracing::debug!("replay buffer stopped");
    }
    guard.starting = false;
    guard.stopping = false;
    guard.last_stopped_at = Some(Instant::now());
    drop(guard);
    REPLAY_BUFFER_LIFECYCLE_CV.notify_all();
}

/// Register a pending plugin save and return the generation to wait past.
/// Incrementing before the save call (so an immediate event still counts as ours)
/// lets [`on_replay_saved`] tell our saves from the user's manual ones.
fn begin_replay_save_request() -> u64 {
    let mut guard = REPLAY_SAVED.lock().unwrap_or_else(|p| p.into_inner());
    guard.pending_requests = guard.pending_requests.saturating_add(1);
    guard.generation
}

#[derive(Debug, PartialEq, Eq)]
enum ReplaySaveWait {
    Saved(Option<String>),
    TimedOut,
}

/// Block until a replay-saved event newer than `since` arrives. A slow save is
/// warned about without abandoning it; only the hard timeout releases ownership.
fn wait_for_replay_saved(since: u64, slow_warning: Duration, timeout: Duration) -> ReplaySaveWait {
    let start = Instant::now();
    let mut warned = false;
    let mut guard = REPLAY_SAVED.lock().unwrap_or_else(|p| p.into_inner());
    while guard.generation == since {
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            // Our event never arrived; release the request so a later user save
            // isn't mistaken for it. `on_replay_saved` holds the same lock, so a
            // just-claimed event would have advanced `generation` and exited above.
            guard.pending_requests = guard.pending_requests.saturating_sub(1);
            return ReplaySaveWait::TimedOut;
        }

        if !warned && elapsed >= slow_warning {
            tracing::warn!(?elapsed, ?timeout, "OBS replay buffer save is slow; continuing to wait");
            warned = true;
        }

        let next_deadline = if warned { timeout } else { slow_warning.min(timeout) };
        let wait_for = next_deadline.saturating_sub(elapsed);
        let (next, _) = REPLAY_SAVED_CV.wait_timeout(guard, wait_for).unwrap_or_else(|p| p.into_inner());
        guard = next;
    }
    ReplaySaveWait::Saved(guard.last_path.clone())
}

fn wait_for_replay_buffer_not_stopping(timeout: Duration) -> bool {
    let start = Instant::now();
    loop {
        let mut guard = REPLAY_BUFFER_LIFECYCLE.lock().unwrap_or_else(|p| p.into_inner());
        while guard.stopping {
            let elapsed = start.elapsed();
            if elapsed >= timeout {
                return false;
            }

            tracing::info!("waiting for replay buffer to finish stopping");
            let (next, res) =
                REPLAY_BUFFER_LIFECYCLE_CV.wait_timeout(guard, timeout - elapsed).unwrap_or_else(|p| p.into_inner());
            guard = next;
            if res.timed_out() && guard.stopping {
                return false;
            }
        }

        let settle_remaining =
            guard.last_stopped_at.and_then(|stopped_at| REPLAY_STOP_SETTLE_DELAY.checked_sub(stopped_at.elapsed()));
        drop(guard);

        if let Some(remaining) = settle_remaining {
            tracing::debug!(?remaining, "letting replay buffer stop settle before restart");
            std::thread::sleep(remaining);
            continue;
        }

        return true;
    }
}

fn wait_for_replay_buffer_active(timeout: Duration) -> bool {
    let start = Instant::now();
    let mut guard = REPLAY_BUFFER_LIFECYCLE.lock().unwrap_or_else(|p| p.into_inner());
    while !replay_buffer_active() {
        if guard.stopping {
            guard.starting = false;
            return false;
        }

        let elapsed = start.elapsed();
        if elapsed >= timeout {
            guard.starting = false;
            return false;
        }

        tracing::info!("waiting for replay buffer to start");
        let (next, res) =
            REPLAY_BUFFER_LIFECYCLE_CV.wait_timeout(guard, timeout - elapsed).unwrap_or_else(|p| p.into_inner());
        guard = next;
        if res.timed_out() && !replay_buffer_active() {
            guard.starting = false;
            return false;
        }
    }

    guard.starting = false;
    guard.last_stopped_at = None;
    true
}

/// Whether the replay buffer is enabled in the active profile (the OBS "Enable
/// Replay Buffer" checkbox). Distinct from [`replay_buffer_active`].
pub fn replay_buffer_enabled() -> bool {
    crate::obs::replay_buffer_enabled()
}

/// Whether OBS currently exposes a replay-buffer output. This can be false even
/// when the checkbox is enabled, for output modes where OBS disables replay
/// buffer internally.
pub fn replay_buffer_available() -> bool {
    crate::obs::replay_buffer_available()
}

/// Configured maximum replay-buffer duration in seconds.
pub fn replay_buffer_max_seconds() -> Option<u64> {
    crate::obs::replay_buffer_max_seconds()
}

/// Directory OBS is configured to write replay-buffer files into.
pub fn replay_buffer_output_directory() -> Option<PathBuf> {
    crate::obs::replay_buffer_output_directory()
}

/// Whether the replay buffer output is currently running.
pub fn replay_buffer_active() -> bool {
    crate::obs::replay_buffer_active()
}

/// Start the replay buffer if it is available and not already running.
pub fn ensure_replay_buffer_running() -> bool {
    let _ensure_guard = REPLAY_BUFFER_ENSURE.lock().unwrap_or_else(|p| p.into_inner());

    if !wait_for_replay_buffer_not_stopping(REPLAY_STOP_TIMEOUT) {
        tracing::warn!("timed out waiting for replay buffer to stop");
        return false;
    }

    if !replay_buffer_available() {
        if replay_buffer_enabled() {
            tracing::warn!("replay buffer is enabled in OBS but unavailable with the current output settings");
        } else {
            tracing::warn!("replay buffer is not enabled in OBS; recording will not work");
        }
        return false;
    }
    if !replay_buffer_active() {
        for attempt in 1..=REPLAY_START_RETRIES {
            tracing::info!(attempt, "starting replay buffer");
            on_replay_buffer_starting();
            crate::obs::start_replay_buffer();
            if wait_for_replay_buffer_active(REPLAY_START_TIMEOUT) {
                return true;
            }
            tracing::warn!(attempt, "replay buffer did not become active after start request");
            std::thread::sleep(REPLAY_START_RETRY_DELAY);
        }
        return false;
    }
    true
}

#[cfg(not(test))]
fn ensure_replay_buffer_running_for_recording() -> bool {
    ensure_replay_buffer_running()
}

#[cfg(test)]
fn ensure_replay_buffer_running_for_recording() -> bool {
    true
}

/// Stop the replay buffer if it is currently running.
pub fn stop_replay_buffer_if_active() {
    if replay_buffer_active() {
        tracing::info!("stopping replay buffer");
        on_replay_buffer_stopping();
        crate::obs::stop_replay_buffer();
    }
}
