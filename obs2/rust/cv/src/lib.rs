//! GoldenEye screen, black-frame, and in-game watch detection for BGRA capture
//! frames. The crate is independent of OBS and the plugin runtime.

mod black_frame;
mod calibration;
mod config;
mod match_result;
mod matcher;
mod overlay_reading;
mod parallel;
mod template_matching;
mod timer;
mod watch;

pub use black_frame::{ActivePictureRegion, BlackFrameSignal, detect_black_frame};
pub use calibration::{CaptureRegion, WORK_HEIGHT};
pub use config::{RuntimeConfig, configure, set_template_dir, template_dir};
pub use match_result::{AnnotationRect, AnnotationSet, LevelMatch, MatchRegion, Screen};
pub use matcher::{CvMatcher, match_level};
pub use timer::PhaseTimer;
pub use watch::{WatchDetector, WatchPresentation, WatchSignal, WatchState, WatchTransition, detect_watch};

pub type Result<T> = opencv::Result<T>;

#[cfg(test)]
#[path = "cv_test.rs"]
mod cv_test;
