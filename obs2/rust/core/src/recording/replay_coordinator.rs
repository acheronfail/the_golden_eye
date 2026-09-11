//! Owns replay lifecycle transitions and serializes identity-less save requests.

use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// How long a monitor start should wait for OBS to finish an in-progress replay
/// buffer stop before giving up.
const REPLAY_STOP_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a monitor start should wait for OBS to make the replay buffer active
/// after `obs_frontend_replay_buffer_start`.
const REPLAY_START_TIMEOUT: Duration = Duration::from_secs(2);
const REPLAY_START_RETRIES: usize = 4;
const REPLAY_START_RETRY_DELAY: Duration = Duration::from_millis(250);
/// OBS can ignore a replay-buffer start issued immediately after the stopped
/// event. Give the frontend a brief turn to finish its state transition.
const REPLAY_STOP_SETTLE_DELAY: Duration = Duration::from_millis(400);
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

struct ReplayBufferLifecycle {
    starting: bool,
    stopping: bool,
    last_stopped_at: Option<Instant>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ReplaySaveWait {
    Saved(Option<String>),
    TimedOut,
}

/// One coordinator per core lifetime; callbacks never acquire either operation gate.
pub(super) struct ReplayCoordinator {
    saved: Mutex<ReplaySaved>,
    saved_changed: Condvar,
    lifecycle: Mutex<ReplayBufferLifecycle>,
    lifecycle_changed: Condvar,
    ensure_serialized: Mutex<()>,
    save_serialized: Mutex<()>,
}

impl ReplayCoordinator {
    pub(super) const fn new() -> Self {
        Self {
            saved: Mutex::new(ReplaySaved { generation: 0, last_path: None, pending_requests: 0 }),
            saved_changed: Condvar::new(),
            lifecycle: Mutex::new(ReplayBufferLifecycle { starting: false, stopping: false, last_stopped_at: None }),
            lifecycle_changed: Condvar::new(),
            ensure_serialized: Mutex::new(()),
            save_serialized: Mutex::new(()),
        }
    }

    pub(super) fn acquire_save(&self) -> ReplaySavePermit<'_> {
        ReplaySavePermit {
            coordinator: self,
            _serialize: self.save_serialized.lock().unwrap_or_else(|p| p.into_inner()),
        }
    }

    /// Publish a replay-saved event and wake any waiting save thread. Called (via
    /// the `ge_replay_buffer_saved` FFI export) from the OBS frontend event
    /// callback when `OBS_FRONTEND_EVENT_REPLAY_BUFFER_SAVED` fires.
    pub(super) fn on_replay_saved(&self, path: Option<String>) {
        let mut guard = self.saved.lock().unwrap_or_else(|p| p.into_inner());
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
        self.saved_changed.notify_all();
    }

    /// Publish that OBS has begun starting the replay buffer.
    pub(super) fn on_replay_buffer_starting(&self) {
        let mut guard = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        if !guard.starting {
            tracing::debug!("replay buffer starting");
        }
        guard.starting = true;
        drop(guard);
        self.lifecycle_changed.notify_all();
    }

    /// Publish that OBS has made the replay buffer active.
    pub(super) fn on_replay_buffer_started(&self) {
        let mut guard = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        if guard.starting {
            tracing::debug!("replay buffer started");
        }
        guard.starting = false;
        guard.last_stopped_at = None;
        drop(guard);
        self.lifecycle_changed.notify_all();
    }

    /// Publish that OBS has begun stopping the replay buffer. This is also called
    /// when we request a stop, because a quick monitor restart can reach
    /// `/monitor/start` before OBS emits the frontend `STOPPING` event.
    pub(super) fn on_replay_buffer_stopping(&self) {
        let mut guard = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        if !guard.stopping {
            tracing::debug!("replay buffer stopping");
        }
        guard.starting = false;
        guard.stopping = true;
        guard.last_stopped_at = None;
        drop(guard);
        self.lifecycle_changed.notify_all();
    }

    /// Publish that OBS has fully stopped the replay buffer and wake any monitor
    /// start waiting to re-enable it.
    pub(super) fn on_replay_buffer_stopped(&self) {
        let mut guard = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        if guard.stopping {
            tracing::debug!("replay buffer stopped");
        }
        guard.starting = false;
        guard.stopping = false;
        guard.last_stopped_at = Some(Instant::now());
        drop(guard);
        self.lifecycle_changed.notify_all();
    }

    /// Register a pending plugin save and return the generation to wait past.
    /// Incrementing before the save call (so an immediate event still counts as ours)
    /// lets [`on_replay_saved`] tell our saves from the user's manual ones.
    fn begin_replay_save_request(&self) -> u64 {
        let mut guard = self.saved.lock().unwrap_or_else(|p| p.into_inner());
        guard.pending_requests = guard.pending_requests.saturating_add(1);
        guard.generation
    }

    /// Block until a replay-saved event newer than `since` arrives. A slow save is
    /// warned about without abandoning it; only the hard timeout releases ownership.
    fn wait_for_replay_saved(&self, since: u64, slow_warning: Duration, timeout: Duration) -> ReplaySaveWait {
        let start = Instant::now();
        let mut warned = false;
        let mut guard = self.saved.lock().unwrap_or_else(|p| p.into_inner());
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
            let (next, _) = self.saved_changed.wait_timeout(guard, wait_for).unwrap_or_else(|p| p.into_inner());
            guard = next;
        }
        ReplaySaveWait::Saved(guard.last_path.clone())
    }

    fn wait_for_replay_buffer_not_stopping(&self, timeout: Duration) -> bool {
        let start = Instant::now();
        loop {
            let mut guard = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
            while guard.stopping {
                let elapsed = start.elapsed();
                if elapsed >= timeout {
                    return false;
                }

                tracing::info!("waiting for replay buffer to finish stopping");
                let (next, res) =
                    self.lifecycle_changed.wait_timeout(guard, timeout - elapsed).unwrap_or_else(|p| p.into_inner());
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

    fn wait_for_replay_buffer_active(&self, timeout: Duration) -> bool {
        let start = Instant::now();
        let mut guard = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        while !crate::obs::replay_buffer_active() {
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
                self.lifecycle_changed.wait_timeout(guard, timeout - elapsed).unwrap_or_else(|p| p.into_inner());
            guard = next;
            if res.timed_out() && !crate::obs::replay_buffer_active() {
                guard.starting = false;
                return false;
            }
        }

        guard.starting = false;
        guard.last_stopped_at = None;
        true
    }

    /// Start the replay buffer if it is available and not already running.
    pub(super) fn ensure_replay_buffer_running(&self) -> bool {
        let _ensure_guard = self.ensure_serialized.lock().unwrap_or_else(|p| p.into_inner());

        if !self.wait_for_replay_buffer_not_stopping(REPLAY_STOP_TIMEOUT) {
            tracing::warn!("timed out waiting for replay buffer to stop");
            return false;
        }

        if !crate::obs::replay_buffer_available() {
            if crate::obs::replay_buffer_enabled() {
                tracing::warn!("replay buffer is enabled in OBS but unavailable with the current output settings");
            } else {
                tracing::warn!("replay buffer is not enabled in OBS; recording will not work");
            }
            return false;
        }
        if !crate::obs::replay_buffer_active() {
            for attempt in 1..=REPLAY_START_RETRIES {
                tracing::info!(attempt, "starting replay buffer");
                self.on_replay_buffer_starting();
                crate::obs::start_replay_buffer();
                if self.wait_for_replay_buffer_active(REPLAY_START_TIMEOUT) {
                    return true;
                }
                tracing::warn!(attempt, "replay buffer did not become active after start request");
                std::thread::sleep(REPLAY_START_RETRY_DELAY);
            }
            return false;
        }
        true
    }

    /// Stop the replay buffer if it is currently running.
    pub(super) fn stop_replay_buffer_if_active(&self) {
        if crate::obs::replay_buffer_active() {
            tracing::info!("stopping replay buffer");
            self.on_replay_buffer_stopping();
            crate::obs::stop_replay_buffer();
        }
    }
}

/// Holds save serialization through completion and file identification, before trimming.
#[must_use = "Keep the permit until the saved replay file has been identified."]
pub(super) struct ReplaySavePermit<'a> {
    coordinator: &'a ReplayCoordinator,
    _serialize: MutexGuard<'a, ()>,
}

impl ReplaySavePermit<'_> {
    pub(super) fn save_and_wait(
        &mut self,
        request: impl FnOnce(),
        slow_warning: Duration,
        timeout: Duration,
    ) -> ReplaySaveWait {
        let since = self.coordinator.begin_replay_save_request();
        request();
        self.coordinator.wait_for_replay_saved(since, slow_warning, timeout)
    }
}

#[cfg(test)]
#[path = "tests/replay_coordinator.rs"]
mod tests;
