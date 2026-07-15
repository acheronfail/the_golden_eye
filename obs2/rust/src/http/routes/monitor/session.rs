use std::collections::VecDeque;
use tokio::sync::broadcast;

use crate::cv::{CaptureRegion, CvMatcher, LevelMatch, Screen};
use crate::ge;
use crate::http::MonitorEvent;

/// Frames voted over to steady the stats times shown live. The per-frame matcher
/// can misread a look-alike digit on a single noisy capture frame; ~7 frames at
/// 60fps hides that (~0.1s lag) without noticeably delaying a real change.
const MONITOR_TIME_SMOOTHING_WINDOW: usize = 7;

/// Sliding-window majority vote over the stats times shown live, voting each
/// field (run / target / best) independently so a single-frame digit misread is
/// outvoted before it reaches the UI. The window resets whenever the on-screen
/// level/screen identity changes, so a lone frame of a fast transition is still
/// shown as-is (it simply votes with a window of one).
pub(super) struct DisplayTimeSmoother {
    key: Option<(Screen, i32, i32, i32)>,
    window: VecDeque<ge::Times>,
}

impl DisplayTimeSmoother {
    pub(super) fn new() -> Self {
        Self { key: None, window: VecDeque::with_capacity(MONITOR_TIME_SMOOTHING_WINDOW) }
    }

    /// Feeds one frame's reading in and returns the smoothed times to display.
    pub(super) fn smooth(&mut self, m: &LevelMatch) -> Option<ge::Times> {
        let key = Some((m.screen, m.mission, m.part, m.difficulty));
        if key != self.key {
            self.key = key;
            self.window.clear();
        }
        let Some(times) = m.times else {
            self.window.clear();
            return None;
        };
        if self.window.len() == MONITOR_TIME_SMOOTHING_WINDOW {
            self.window.pop_front();
        }
        self.window.push_back(times);
        Some(ge::Times {
            time: self.majority(|t| t.time)?,
            target_time: self.majority(|t| t.target_time)?,
            best_time: self.majority(|t| t.best_time)?,
        })
    }

    /// Most-common value of `field` across the window, ties to the newest frame.
    fn majority<T: PartialEq + Copy>(&self, field: impl Fn(&ge::Times) -> T) -> Option<T> {
        let mut best: Option<(T, usize)> = None;
        for cand in self.window.iter().rev() {
            let v = field(cand);
            let count = self.window.iter().filter(|t| field(t) == v).count();
            if best.is_none_or(|(_, bc)| count > bc) {
                best = Some((v, count));
            }
        }
        best.map(|(v, _)| v)
    }
}

/// Source of frames for fixture-backed monitor-loop tests. The live OBS source
/// uses `ObsSource::capture_with_stats_until` so production can carry timing metadata.
#[cfg(test)]
pub trait FrameSource {
    /// Acquire the next BGRA frame and hand it to `use_frame`. Returns the
    /// closure's value, or `None` when no frame is available right now.
    fn capture<F, R>(&mut self, use_frame: F) -> Option<R>
    where
        F: FnOnce(&[u8], u32, u32) -> R;

    /// Offer the source a capture transform the matcher has learned, so it can
    /// have the GPU crop + un-stretch future frames at capture time. Sources
    /// that can't reshape their frames (test fixtures) ignore it.
    fn set_capture_region(&mut self, _region: Option<CaptureRegion>) {}
}

/// A monitor session: owns the matcher (and its per-resolution scale cache) for
/// one start/stop cycle, so dropping the session clears the cache and each start
/// begins cold. The cache keys on source dimensions, re-learning on resolution changes.
pub struct MonitorSession {
    matcher: CvMatcher,
}

impl MonitorSession {
    /// Builds a session with the given language, using the bundled CV templates
    /// directory resolved at plugin startup.
    pub fn from_env(lang: &str) -> anyhow::Result<Self> {
        let template_dir =
            crate::cv::template_dir().ok_or_else(|| anyhow::anyhow!("CV template directory is not set"))?;
        Self::new(lang, &template_dir)
    }

    /// Builds a session with an explicit language and template directory.
    pub fn new(lang: &str, template_dir: &str) -> anyhow::Result<Self> {
        let matcher = CvMatcher::new(lang, template_dir)
            .map_err(|err| anyhow::anyhow!("failed to init matcher: {}", err.message))?;
        Ok(MonitorSession { matcher })
    }

    pub fn with_diagnostics(mut self, enabled: bool) -> Self {
        self.matcher.set_diagnostics(enabled);
        self
    }

    pub fn set_diagnostics(&mut self, enabled: bool) {
        self.matcher.set_diagnostics(enabled);
    }

    pub(super) fn capture_region(&self) -> Option<CaptureRegion> {
        self.matcher.capture_region()
    }

    /// Matches one BGRA frame. The matcher's scale cache makes the first overlay
    /// frame at a given resolution costlier (it searches for the scale) and every
    /// later frame at that resolution cheap (it reuses the learned scale).
    pub fn match_frame(&self, bytes: &[u8], width: u32, height: u32) -> opencv::Result<LevelMatch> {
        self.matcher.match_level_from_bgra_bytes(bytes, width, height)
    }

    /// Hot loop used by tests: take each frame `source` yields, match it, and
    /// pass the result to `on_result`.
    #[cfg(test)]
    pub fn run<S, F>(&self, source: &mut S, mut on_result: F)
    where
        S: FrameSource,
        F: FnMut(opencv::Result<LevelMatch>),
    {
        while let Some(result) = source.capture(|bytes, w, h| self.match_frame(bytes, w, h)) {
            // Once the matcher has calibrated this source's aspect, hand the
            // transform to the capture layer so subsequent frames are cropped +
            // un-stretched on the GPU at capture time.
            source.set_capture_region(self.matcher.capture_region());
            on_result(result);
        }
    }
}

pub(super) fn handle_detected_language(
    info: &LevelMatch,
    session: &mut MonitorSession,
    active_lang: &mut String,
    language_notified: &mut bool,
    event_tx: &broadcast::Sender<MonitorEvent>,
    make_session: impl FnOnce(&str) -> anyhow::Result<MonitorSession>,
) -> bool {
    let Some(detected_lang) = info.detected_lang.as_deref().map(str::to_owned) else {
        return false;
    };

    if detected_lang == *active_lang {
        if !*language_notified {
            tracing::info!(detected_lang, "detected ROM language");
            *language_notified = true;
            let _ = event_tx.send(MonitorEvent::LanguageDetected { lang: active_lang.clone() });
        }
        return false;
    }

    tracing::info!(
        active_lang = %active_lang,
        detected_lang,
        "detected ROM language; switching monitor templates"
    );
    match make_session(&detected_lang) {
        Ok(next_session) => {
            *session = next_session;
            *active_lang = detected_lang.clone();
        }
        Err(err) => {
            tracing::error!(detected_lang, "failed to switch monitor language after detection: {err}");
            return false;
        }
    }

    // A real language switch is always worth re-notifying, even if we already
    // notified for the previous language this session.
    *language_notified = true;
    let _ = event_tx.send(MonitorEvent::LanguageDetected { lang: active_lang.clone() });

    true
}

pub(super) fn log_level_match(info: &LevelMatch) {
    match serde_json::to_string(info) {
        Ok(json) => tracing::info!("{json}"),
        Err(err) => tracing::info!(?info, "failed to serialize level match as JSON: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use opencv::prelude::*;
    use opencv::{imgcodecs, imgproc};

    use super::*;
    use crate::ge::Times;

    // Builds the classified stats-screen times for a test expectation.
    const fn times(time: i32, target_time: Option<i32>, best_time: Option<i32>) -> Option<Times> {
        Some(Times { time, target_time, best_time })
    }

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

    // A minimal stats-screen match for the display smoother; only the identity
    // fields and `times` matter to it.
    fn stats_frame(mission: i32, times: Option<Times>) -> LevelMatch {
        LevelMatch {
            screen: Screen::Stats,
            mission,
            part: 1,
            difficulty: 2,
            detected_lang: None,
            times,
            raw_times: Vec::new(),
            match_regions: Vec::new(),
            annotation_sets: Vec::new(),
            runtime_ms: 0.0,
        }
    }

    #[test]
    fn display_smoother_outvotes_a_single_frame_best_time_flicker() {
        let mut smoother = DisplayTimeSmoother::new();
        let mut out = None;
        // 28 is stable apart from a lone 20 flicker; the majority holds even on the
        // flicker frame and at the end (whose last frame also read 20).
        for best in [Some(28), Some(28), Some(20), Some(28), Some(28), Some(20)] {
            out = smoother.smooth(&stats_frame(1, times(28, Some(300), best)));
        }
        assert_eq!(out, times(28, Some(300), Some(28)));
    }

    #[test]
    fn display_smoother_passes_a_lone_frame_through() {
        // A fast transition may only ever yield one frame; it is shown as read.
        let mut smoother = DisplayTimeSmoother::new();
        let out = smoother.smooth(&stats_frame(1, times(28, Some(300), Some(20))));
        assert_eq!(out, times(28, Some(300), Some(20)));
    }

    #[test]
    fn display_smoother_resets_on_level_change() {
        let mut smoother = DisplayTimeSmoother::new();
        for _ in 0..4 {
            smoother.smooth(&stats_frame(1, times(28, Some(300), Some(28))));
        }
        // A different level's window must start fresh, not inherit the old votes.
        let out = smoother.smooth(&stats_frame(2, times(50, Some(300), Some(40))));
        assert_eq!(out, times(50, Some(300), Some(40)));
    }

    #[test]
    fn display_smoother_votes_each_field_independently() {
        let mut smoother = DisplayTimeSmoother::new();
        let mut out = None;
        // Run time flickers while best/target stay put: only the run time is voted.
        for time in [28, 28, 61, 28, 28] {
            out = smoother.smooth(&stats_frame(1, times(time, Some(300), Some(28))));
        }
        assert_eq!(out, times(28, Some(300), Some(28)));
    }

    /// Frame source that replays decoded fixtures, returning `None` once the
    /// stream is exhausted so a `run` loop exits.
    struct FixtureSource {
        frames: Vec<(Vec<u8>, u32, u32)>,
        idx: usize,
    }

    impl FrameSource for FixtureSource {
        fn capture<F, R>(&mut self, use_frame: F) -> Option<R>
        where
            F: FnOnce(&[u8], u32, u32) -> R,
        {
            let (bytes, w, h) = self.frames.get(self.idx)?;
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
        times: Option<Times>,
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
            times: None,
        },
        Case {
            file: "screenshots-av2hdmi/en - start - 16 - Secret Agent.png",
            lang: "en",
            mission: 7,
            part: 2,
            difficulty: 1,
            times: None,
        },
        Case {
            // Dam on Agent; Dam's target is set for Secret Agent, so no target
            // row shows here -- the second time is the best time.
            file: "screenshots-av2hdmi/en - stats - 01 - Agent - 0119_0119.png",
            lang: "en",
            mission: 1,
            part: 1,
            difficulty: 0,
            times: times(79, None, Some(79)),
        },
        Case {
            // Archives on Agent; its target is set for 00 Agent, so no target row.
            file: "screenshots-av2hdmi/en - stats - 11 - Agent - 0043_0043.png",
            lang: "en",
            mission: 6,
            part: 2,
            difficulty: 0,
            times: times(43, None, Some(43)),
        },
        Case {
            file: "screenshots-emu/en - start - 20 - Agent.png",
            lang: "en",
            mission: 9,
            part: 1,
            difficulty: 0,
            times: None,
        },
        Case {
            // Runway on Agent; its target IS set for Agent, so the target row
            // shows (middle time), followed by the best time.
            file: "screenshots-emu/en - stats - 03 - Agent - 0033_0500_0033.png",
            lang: "en",
            mission: 1,
            part: 3,
            difficulty: 0,
            times: times(33, Some(300), Some(33)),
        },
        Case {
            file: "screenshots-emu/jp - start - 01 - 00 Agent.png",
            lang: "jp",
            mission: 1,
            part: 1,
            difficulty: 2,
            times: None,
        },
        Case {
            // Dam on Agent (jp); target is Secret Agent, so no target row.
            file: "screenshots-emu/jp - stats - 01 - Agent - 0137_0137.png",
            lang: "jp",
            mission: 1,
            part: 1,
            difficulty: 0,
            times: times(97, None, Some(97)),
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
    fn start_screen_language_mismatch_is_detected_and_rejected() {
        let cases = [
            ("jp", "en", "screenshots-emu/en - start - 01 - Agent.png"),
            ("en", "jp", "screenshots-emu/jp - start - 01 - Agent.png"),
            ("jp", "en", "screenshots-av2hdmi/en - start - 3 - 00 Agent - blackbars.png"),
        ];

        for (configured, detected, file) in cases {
            let session = MonitorSession::new(configured, TEMPLATES_DIR).expect("session");
            let (bytes, w, h) = load_bgra(file);
            let m = session.match_frame(&bytes, w, h).expect("match");
            assert_eq!(m.detected_lang.as_deref(), Some(detected), "{file} detected language");
            assert_eq!(m.screen, crate::cv::Screen::Unknown, "{file} screen");
            assert_eq!(m.raw_times, Vec::<i32>::new(), "{file} raw times");
            assert_eq!(m.times, None, "{file} times");
        }
    }

    #[test]
    fn detected_language_switches_active_monitor_language_and_notifies_once() {
        let mut session = MonitorSession::new("en", TEMPLATES_DIR).expect("session");
        let mut active_lang = "en".to_owned();
        let mut language_notified = false;
        let (event_tx, mut event_rx) = broadcast::channel(8);

        let (start_b, start_w, start_h) = load_bgra("screenshots-emu/jp - start - 01 - Agent.png");
        let mismatch = session.match_frame(&start_b, start_w, start_h).expect("mismatch match");
        assert_eq!(mismatch.detected_lang.as_deref(), Some("jp"));
        assert_eq!(mismatch.screen, crate::cv::Screen::Unknown);

        let switched = handle_detected_language(
            &mismatch,
            &mut session,
            &mut active_lang,
            &mut language_notified,
            &event_tx,
            |lang| MonitorSession::new(lang, TEMPLATES_DIR),
        );

        assert!(switched, "mismatch should switch the active matcher");
        assert_eq!(active_lang, "jp");
        assert!(language_notified);

        let event = event_rx.try_recv().expect("language detected event");
        assert!(matches!(event, MonitorEvent::LanguageDetected { lang } if lang == "jp"));

        let (stats_b, stats_w, stats_h) = load_bgra("screenshots-emu/jp - stats - 01 - Agent - 0137_0137.png");
        let stats = session.match_frame(&stats_b, stats_w, stats_h).expect("jp stats after switch");
        assert_eq!(stats.screen, crate::cv::Screen::Stats);
        assert_eq!(stats.mission, 1);
        assert_eq!(stats.part, 1);
        assert_eq!(stats.difficulty, 0);
        assert_eq!(stats.times, times(97, None, Some(97)));

        let repeated = handle_detected_language(
            &mismatch,
            &mut session,
            &mut active_lang,
            &mut language_notified,
            &event_tx,
            |lang| MonitorSession::new(lang, TEMPLATES_DIR),
        );
        assert!(!repeated, "already-active detected language should not switch again");
        assert!(matches!(event_rx.try_recv(), Err(broadcast::error::TryRecvError::Empty)));
    }

    #[test]
    fn detected_language_notifies_when_already_active() {
        let mut session = MonitorSession::new("en", TEMPLATES_DIR).expect("session");
        let mut active_lang = "en".to_owned();
        let mut language_notified = false;
        let (event_tx, mut event_rx) = broadcast::channel(8);

        let (start_b, start_w, start_h) = load_bgra("screenshots-emu/en - start - 01 - Agent.png");
        let detected = session.match_frame(&start_b, start_w, start_h).expect("detected match");
        assert_eq!(detected.detected_lang.as_deref(), Some("en"));

        let switched = handle_detected_language(
            &detected,
            &mut session,
            &mut active_lang,
            &mut language_notified,
            &event_tx,
            |lang| MonitorSession::new(lang, TEMPLATES_DIR),
        );

        assert!(!switched, "already-active detected language should not switch");
        assert_eq!(active_lang, "en");
        assert!(language_notified);
        let event = event_rx.try_recv().expect("language detected event");
        assert!(matches!(event, MonitorEvent::LanguageDetected { lang } if lang == "en"));
    }

    #[test]
    fn detected_language_can_switch_more_than_once_per_monitor_session() {
        let mut session = MonitorSession::new("en", TEMPLATES_DIR).expect("session");
        let mut active_lang = "en".to_owned();
        let mut language_notified = false;
        let (event_tx, mut event_rx) = broadcast::channel(8);

        let (en_b, en_w, en_h) = load_bgra("screenshots-emu/en - start - 01 - Agent.png");
        let en_detected = session.match_frame(&en_b, en_w, en_h).expect("en match");
        assert_eq!(en_detected.detected_lang.as_deref(), Some("en"));

        let first = handle_detected_language(
            &en_detected,
            &mut session,
            &mut active_lang,
            &mut language_notified,
            &event_tx,
            |lang| MonitorSession::new(lang, TEMPLATES_DIR),
        );
        assert!(!first, "initial same-language detection should not switch");
        assert_eq!(active_lang, "en");
        assert!(language_notified);
        let event = event_rx.try_recv().expect("language detected event");
        assert!(matches!(event, MonitorEvent::LanguageDetected { lang } if lang == "en"));

        let (jp_b, jp_w, jp_h) = load_bgra("screenshots-emu/jp - start - 01 - Agent.png");
        let jp_mismatch = session.match_frame(&jp_b, jp_w, jp_h).expect("jp mismatch match");
        assert_eq!(jp_mismatch.detected_lang.as_deref(), Some("jp"));

        let switched_to_jp = handle_detected_language(
            &jp_mismatch,
            &mut session,
            &mut active_lang,
            &mut language_notified,
            &event_tx,
            |lang| MonitorSession::new(lang, TEMPLATES_DIR),
        );
        assert!(switched_to_jp, "language change should switch after notification");
        assert_eq!(active_lang, "jp");
        let event = event_rx.try_recv().expect("language detected event on switch");
        assert!(matches!(event, MonitorEvent::LanguageDetected { lang } if lang == "jp"));

        let en_mismatch = session.match_frame(&en_b, en_w, en_h).expect("en mismatch match");
        assert_eq!(en_mismatch.detected_lang.as_deref(), Some("en"));

        let switched_back_to_en = handle_detected_language(
            &en_mismatch,
            &mut session,
            &mut active_lang,
            &mut language_notified,
            &event_tx,
            |lang| MonitorSession::new(lang, TEMPLATES_DIR),
        );
        assert!(switched_back_to_en, "a second language change should still switch");
        assert_eq!(active_lang, "en");
        let event = event_rx.try_recv().expect("language detected event on switch back");
        assert!(matches!(event, MonitorEvent::LanguageDetected { lang } if lang == "en"));
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
        assert_eq!(cold.times, times(79, None, Some(79)));
        assert_eq!(warm.times, cold.times);
        assert_eq!((warm.mission, warm.part), (cold.mission, cold.part));

        // A different resolution in the same session is keyed separately, so the
        // 480p cache never corrupts the 1080p read, and vice versa.
        let other = session.match_frame(&run_b, run_w, run_h).expect("other res");
        assert_eq!(other.times, times(33, Some(300), Some(33)));
        let back = session.match_frame(&dam_b, dam_w, dam_h).expect("back");
        assert_eq!(back.times, times(79, None, Some(79)));

        // A fresh session starts cold and reproduces the result exactly,
        // confirming the cache is owned per-session (cleared on stop).
        let session2 = MonitorSession::new("en", TEMPLATES_DIR).expect("session2");
        let fresh = session2.match_frame(&dam_b, dam_w, dam_h).expect("fresh");
        assert_eq!(fresh.times, times(79, None, Some(79)));
    }

    #[test]
    fn run_processes_a_frame_stream_until_exhausted() {
        let files = [
            "screenshots-emu/en - start - 20 - Agent.png",
            "screenshots-emu/en - stats - 03 - Agent - 0033_0500_0033.png",
            "screenshots-av2hdmi/en - start - 08 - Agent.png",
        ];
        let frames: Vec<_> = files.iter().map(|f| load_bgra(f)).collect();

        let mut source = FixtureSource { frames, idx: 0 };
        let session = MonitorSession::new("en", TEMPLATES_DIR).expect("session");

        let mut results = Vec::new();
        session.run(&mut source, |r| results.push(r.expect("match")));

        assert_eq!(results.len(), 3, "every fixture frame is processed once");
        assert_eq!(results[0].mission, 9); // start 20 -> Egyptian
        assert_eq!(results[1].times, times(33, Some(300), Some(33))); // stats 03 (Runway on Agent: run, target, best)
        assert_eq!(results[2].mission, 5); // start 08 -> Surface 2
    }
}
