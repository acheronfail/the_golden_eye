use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct MatchRegion {
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct AnnotationRect {
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct AnnotationSet {
    pub id: String,
    pub label: String,
    pub annotations: Vec<AnnotationRect>,
}

fn template_annotation(region: &MatchRegion) -> AnnotationRect {
    AnnotationRect {
        label: region.label.clone(),
        x: region.x,
        y: region.y,
        w: region.w,
        h: region.h,
        score: Some(region.score),
    }
}

pub(super) fn annotation_sets(
    watch_detection: Option<AnnotationSet>,
    match_regions: &[MatchRegion],
    search_regions: Vec<AnnotationRect>,
    folder_region: Option<AnnotationRect>,
    time_digits: Vec<AnnotationRect>,
) -> Vec<AnnotationSet> {
    let mut sets = Vec::new();
    if !match_regions.is_empty() {
        sets.push(AnnotationSet {
            id: "template_matches".to_owned(),
            label: "Template matches".to_owned(),
            annotations: match_regions.iter().map(template_annotation).collect(),
        });
    }
    if !time_digits.is_empty() {
        sets.push(AnnotationSet {
            id: "time_digits".to_owned(),
            label: "Time digits".to_owned(),
            annotations: time_digits,
        });
    }
    if !search_regions.is_empty() {
        sets.push(AnnotationSet {
            id: "search_rois".to_owned(),
            label: "Search ROIs".to_owned(),
            annotations: search_regions,
        });
    }
    if let Some(folder_region) = folder_region {
        sets.push(AnnotationSet {
            id: "folder_dimensions".to_owned(),
            label: "Folder dimensions".to_owned(),
            annotations: vec![folder_region],
        });
    }
    if let Some(watch_detection) = watch_detection {
        sets.push(watch_detection);
    }
    sets
}

// Which overlay screen a frame shows. All but `Levels` share the
// mission/part/difficulty header; they are told apart by the banner word below
// it or, for report screens, the status value. `Unknown` covers gameplay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(rename_all = "lowercase")]
pub enum Screen {
    Unknown,
    Start,
    Stats,
    Complete,
    Failed,
    Abort,
    Kia,
    Opts007,
    Select,
    Levels,
}

impl Screen {
    // Strings match the `ScreenshotInfo.screen` values used by the test suite.
    pub fn as_str(self) -> &'static str {
        match self {
            Screen::Unknown => "unknown",
            Screen::Start => "start",
            Screen::Stats => "stats",
            Screen::Complete => "complete",
            Screen::Failed => "failed",
            Screen::Abort => "abort",
            Screen::Kia => "kia",
            Screen::Opts007 => "007opts",
            Screen::Select => "select",
            Screen::Levels => "levels",
        }
    }

    /// Screens that launch a mission and anchor recording/timing state.
    pub fn is_level_launch(self) -> bool {
        matches!(self, Screen::Start | Screen::Opts007)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct LevelMatch {
    pub screen: Screen,
    pub mission: i32,
    pub part: i32,
    pub difficulty: i32,
    /// Game language detected from language-specific static UI, when a strong
    /// signal is visible. Currently emitted on level-start briefing screens.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "\"en\" | \"jp\"")]
    pub detected_lang: Option<String>,
    /// The stats-screen times split into run / target / best (see [`ge_game::Times`]).
    /// `None` on any screen that carries no timed rows (start, report, gameplay).
    pub times: Option<ge_game::Times>,
    /// Raw times read off the overlay top-to-bottom, before classification (the
    /// source `times` derives from). Empty on untimed screens. Kept for the test
    /// harness; production code uses the classified `times` instead.
    #[ts(optional = nullable)]
    pub raw_times: Vec<i32>,
    /// Optional template-match rectangles for developer tooling. These are
    /// empty unless annotation diagnostics are explicitly enabled.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[ts(optional = nullable)]
    pub match_regions: Vec<MatchRegion>,
    /// Developer-only annotation sets. The normal monitor path leaves this
    /// empty so no annotation collection work is done per frame.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[ts(optional = nullable)]
    pub annotation_sets: Vec<AnnotationSet>,
    pub runtime_ms: f64,
}

impl LevelMatch {
    /// Whether this match describes the same on-screen state as `other`,
    /// ignoring `runtime_ms` (the per-frame match cost, which changes every
    /// frame and would otherwise defeat any "only on change" deduplication).
    pub fn same_state(&self, other: &LevelMatch) -> bool {
        self.screen == other.screen
            && self.mission == other.mission
            && self.part == other.part
            && self.difficulty == other.difficulty
            && self.detected_lang == other.detected_lang
            && self.times == other.times
    }
}

fn screen_requires_overlay_markers(screen: Screen) -> bool {
    matches!(screen, Screen::Start | Screen::Stats | Screen::Complete | Screen::Failed | Screen::Abort | Screen::Kia)
}

pub(super) fn has_overlay_markers(result: &LevelMatch) -> bool {
    result.mission >= 0 && result.part >= 0 && result.difficulty >= 0
}

pub(super) fn reject_untrusted_screen(result: &mut LevelMatch) {
    let missing_required_markers = screen_requires_overlay_markers(result.screen) && !has_overlay_markers(result);
    let stats_without_times = result.screen == Screen::Stats && result.raw_times.is_empty();
    // A strong START-tab match identifies the pre-level briefing flow. Briefing
    // text can resemble both the statistics banner and time rows, but real
    // post-run stats screens never carry this tab.
    let stats_with_start_tab = result.screen == Screen::Stats && result.detected_lang.is_some();
    if missing_required_markers || stats_without_times || stats_with_start_tab {
        result.screen = Screen::Unknown;
        result.times = None;
        result.raw_times.clear();
    }
}
