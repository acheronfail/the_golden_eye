use std::ffi::CString;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::Deserialize;

use crate::http::AppState;

/// A running monitor: the worker thread plus the flag used to ask it to stop.
pub struct MonitorHandle {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartParams {
    /// Name of the OBS source to monitor, as reported by `/api/v1/sources`.
    source_name: String,
}

#[axum::debug_handler]
pub async fn handle_start(State(state): State<AppState>, Json(params): Json<StartParams>) -> Result<impl IntoResponse> {
    let source_name =
        CString::new(params.source_name).map_err(|_| (StatusCode::BAD_REQUEST, "source name contains a null byte"))?;

    // Only one monitor may run at a time; reject the request if one already is.
    let mut guard = state.monitor.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_some() {
        return Err((StatusCode::CONFLICT, "a monitor is already running").into());
    }

    // Run the matcher on a dedicated OS thread so its blocking, CPU-bound work
    // never ties up the async runtime's worker threads.
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let thread = std::thread::Builder::new()
        .name("ge-monitor".to_owned())
        .spawn(move || monitor_loop(source_name, thread_stop))
        .map_err(|err| {
            tracing::error!("failed to spawn monitor thread: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to spawn monitor thread")
        })?;

    *guard = Some(MonitorHandle { stop, thread });
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

    // Signal the loop to exit, then wait for it on a blocking thread so we don't
    // stall the async runtime while the in-flight match finishes.
    tokio::task::spawn_blocking(move || {
        handle.stop.store(true, Ordering::Relaxed);
        if handle.thread.join().is_err() {
            tracing::error!("monitor thread panicked");
        }
    })
    .await
    .ok();

    tracing::info!("monitor stopped");

    Ok(StatusCode::OK)
}

/// Hot loop: capture the source frame, run the level matcher, log the result,
/// and repeat until asked to stop.
fn monitor_loop(source_name: CString, stop: Arc<AtomicBool>) {
    let matcher = match crate::cv::CvMatcher::new("en", "/home/acheronfail/src/ge-obs/obs2/cv_templates") {
        Ok(matcher) => matcher,
        Err(err) => {
            tracing::error!("failed to init matcher for monitor: {err}");
            return;
        }
    };

    while !stop.load(Ordering::Relaxed) {
        let s = Instant::now();

        // Render the source into a BGRA buffer owned by the C side.
        let mut width: u32 = 0;
        let mut height: u32 = 0;
        let frame = unsafe { crate::ffi::ge_obs_get_source_frame(source_name.as_ptr(), &mut width, &mut height) };
        if frame.is_null() {
            tracing::warn!("monitor: could not capture source frame");
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }

        let ft = s.elapsed().as_millis();

        let result = matcher.match_level_from_raw_bytes(frame, width, height);
        // Hand the buffer straight back to the C allocator once we're done with it.
        unsafe { crate::ffi::free(frame.cast()) };

        tracing::info!(ft, ?result, "match");
    }

    tracing::info!("monitor loop exiting");
}
