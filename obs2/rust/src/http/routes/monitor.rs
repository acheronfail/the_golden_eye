use std::ffi::CString;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Result};
use serde::Deserialize;

use crate::cv::{CvMatcher, LevelMatch};
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
}

/// Frame source backed by the live OBS source named at construction.
struct ObsSource {
    name: CString,
}

impl FrameSource for ObsSource {
    fn capture<F, R>(&mut self, use_frame: F) -> Option<R>
    where
        F: FnOnce(&[u8], u32, u32) -> R,
    {
        // Render the source into a BGRA buffer owned by the C side.
        let mut width: u32 = 0;
        let mut height: u32 = 0;
        let frame = unsafe { crate::ffi::ge_obs_get_source_frame(self.name.as_ptr(), &mut width, &mut height) };
        if frame.is_null() {
            return None;
        }
        let total_bytes = (width * height * 4) as usize;
        let bytes = unsafe { std::slice::from_raw_parts(frame, total_bytes) };
        let out = use_frame(bytes, width, height);
        // Hand the buffer straight back to the C allocator now we're done with it.
        unsafe { crate::ffi::free(frame.cast()) };
        Some(out)
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
    /// Builds a session, reading `GE_CV_LANG` and `GE_CV_TEMPLATE_DIR` from the
    /// environment (as the rest of the plugin does).
    pub fn from_env() -> anyhow::Result<Self> {
        let lang = std::env::var("GE_CV_LANG").map_err(|_| anyhow::anyhow!("GE_CV_LANG is not set"))?;
        let template_dir =
            std::env::var("GE_CV_TEMPLATE_DIR").map_err(|_| anyhow::anyhow!("GE_CV_TEMPLATE_DIR is not set"))?;
        Self::new(&lang, &template_dir)
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

    /// Hot loop: pull frames from `source`, match each, and pass the result to
    /// `on_result`, until `stop` is set. When no frame is available the loop
    /// backs off briefly before retrying.
    pub fn run<S, F>(&self, source: &mut S, stop: &AtomicBool, mut on_result: F)
    where
        S: FrameSource,
        F: FnMut(opencv::Result<LevelMatch>),
    {
        while !stop.load(Ordering::Relaxed) {
            match source.capture(|bytes, w, h| self.match_frame(bytes, w, h)) {
                Some(result) => on_result(result),
                None => {
                    // No frame this tick. Re-check the stop flag before sleeping
                    // so an exhausted source (e.g. a test fixture that sets the
                    // flag) exits promptly instead of waiting out the backoff.
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
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

    // Build the session (and its fresh, empty scale cache) up front so any
    // configuration error surfaces as a failed request rather than a thread that
    // silently exits.
    let session = MonitorSession::from_env().map_err(|err| {
        tracing::error!("failed to start monitor: {err}");
        (StatusCode::INTERNAL_SERVER_ERROR, "failed to init matcher")
    })?;

    // Run the matcher on a dedicated OS thread so its blocking, CPU-bound work
    // never ties up the async runtime's worker threads. The session is moved
    // onto the thread and dropped when the loop exits, clearing the cache.
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let thread = std::thread::Builder::new()
        .name("ge-monitor".to_owned())
        .spawn(move || {
            let mut source = ObsSource { name: source_name };
            session.run(&mut source, &thread_stop, |result| match result {
                Ok(info) => tracing::info!(?info),
                Err(e) => tracing::error!("err: {}", e.message),
            });
            tracing::info!("monitor loop exiting");
        })
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
    // stall the async runtime while the in-flight match finishes. Joining the
    // thread drops the session, releasing the matcher and its scale cache.
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

#[cfg(test)]
mod tests {
    use opencv::prelude::*;
    use opencv::{imgcodecs, imgproc};

    use super::*;

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

    /// Frame source that replays decoded fixtures, then sets `stop` so a `run`
    /// loop exits once the stream is exhausted.
    struct FixtureSource {
        frames: Vec<(Vec<u8>, u32, u32)>,
        idx: usize,
        stop: Arc<AtomicBool>,
    }

    impl FrameSource for FixtureSource {
        fn capture<F, R>(&mut self, use_frame: F) -> Option<R>
        where
            F: FnOnce(&[u8], u32, u32) -> R,
        {
            let (bytes, w, h) = self.frames.get(self.idx).or_else(|| {
                self.stop.store(true, Ordering::Relaxed);
                None
            })?;
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
        times: &'static [i32],
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
            times: &[],
        },
        Case {
            file: "screenshots-av2hdmi/en - start - 16 - Secret Agent.png",
            lang: "en",
            mission: 7,
            part: 2,
            difficulty: 1,
            times: &[],
        },
        Case {
            file: "screenshots-av2hdmi/en - stats - 01 - Agent - 0119_0119.png",
            lang: "en",
            mission: 1,
            part: 1,
            difficulty: 0,
            times: &[79, 79],
        },
        Case {
            file: "screenshots-av2hdmi/en - stats - 11 - Agent - 0043_0043.png",
            lang: "en",
            mission: 6,
            part: 2,
            difficulty: 0,
            times: &[43, 43],
        },
        Case {
            file: "screenshots-emu/en - start - 20 - Agent.png",
            lang: "en",
            mission: 9,
            part: 1,
            difficulty: 0,
            times: &[],
        },
        Case {
            file: "screenshots-emu/en - stats - 03 - Agent - 0033_0500_0033.png",
            lang: "en",
            mission: 1,
            part: 3,
            difficulty: 0,
            times: &[33, 300, 33],
        },
        Case {
            file: "screenshots-emu/jp - start - 01 - 00 Agent.png",
            lang: "jp",
            mission: 1,
            part: 1,
            difficulty: 2,
            times: &[],
        },
        Case {
            file: "screenshots-emu/jp - stats - 01 - Agent - 0137_0137.png",
            lang: "jp",
            mission: 1,
            part: 1,
            difficulty: 0,
            times: &[97, 97],
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
        assert_eq!(cold.times, vec![79, 79]);
        assert_eq!(warm.times, cold.times);
        assert_eq!((warm.mission, warm.part), (cold.mission, cold.part));

        // A different resolution in the same session is keyed separately, so the
        // 480p cache never corrupts the 1080p read, and vice versa.
        let other = session.match_frame(&run_b, run_w, run_h).expect("other res");
        assert_eq!(other.times, vec![33, 300, 33]);
        let back = session.match_frame(&dam_b, dam_w, dam_h).expect("back");
        assert_eq!(back.times, vec![79, 79]);

        // A fresh session starts cold and reproduces the result exactly,
        // confirming the cache is owned per-session (cleared on stop).
        let session2 = MonitorSession::new("en", TEMPLATES_DIR).expect("session2");
        let fresh = session2.match_frame(&dam_b, dam_w, dam_h).expect("fresh");
        assert_eq!(fresh.times, vec![79, 79]);
    }

    #[test]
    fn run_processes_a_frame_stream_until_exhausted() {
        let files = [
            "screenshots-emu/en - start - 20 - Agent.png",
            "screenshots-emu/en - stats - 03 - Agent - 0033_0500_0033.png",
            "screenshots-av2hdmi/en - start - 08 - Agent.png",
        ];
        let frames: Vec<_> = files.iter().map(|f| load_bgra(f)).collect();

        let stop = Arc::new(AtomicBool::new(false));
        let mut source = FixtureSource { frames, idx: 0, stop: stop.clone() };
        let session = MonitorSession::new("en", TEMPLATES_DIR).expect("session");

        let mut results = Vec::new();
        session.run(&mut source, &stop, |r| results.push(r.expect("match")));

        assert_eq!(results.len(), 3, "every fixture frame is processed once");
        assert_eq!(results[0].mission, 9); // start 20 -> Egyptian
        assert_eq!(results[1].times, vec![33, 300, 33]); // stats 03
        assert_eq!(results[2].mission, 5); // start 08 -> Surface 2
    }
}
