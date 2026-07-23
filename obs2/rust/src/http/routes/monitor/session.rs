use std::collections::VecDeque;

use crate::cv::{CaptureRegion, CvMatcher, LevelMatch, Screen};
use crate::ge;

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

pub(super) fn switch_detected_language(
    info: &LevelMatch,
    session: &mut MonitorSession,
    active_lang: &mut String,
    make_session: impl FnOnce(&str) -> anyhow::Result<MonitorSession>,
) -> bool {
    let Some(detected_lang) = info.detected_lang.as_deref().map(str::to_owned) else {
        return false;
    };

    if detected_lang == *active_lang {
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

    true
}

pub(super) fn log_level_match(info: &LevelMatch) {
    match serde_json::to_string(info) {
        Ok(json) => tracing::info!("{json}"),
        Err(err) => tracing::info!(?info, "failed to serialize level match as JSON: {err}"),
    }
}

#[cfg(test)]
#[path = "session_test.rs"]
mod session_test;
