//! Replay-buffer driven recording. We keep OBS's replay buffer running for the whole
//! session and save/trim (via `ge_media`) a window out of it per run, rather
//! than start/stop per run. Padding is anchored to the save moment (file ends at ~now).

use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, c_char};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use anyhow::Context;
use ge_clip::{ClipMetadata, RunStatus};
pub use ge_settings::{
    DEFAULT_CLIP_FILENAME_TEMPLATE,
    DEFAULT_POST_RUN_PADDING_SECS,
    DEFAULT_PRE_RUN_PADDING_SECS,
    DEFAULT_RECENT_RUN_LIMIT,
    MAX_RECENT_RUN_LIMIT,
};
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::cv::{LevelMatch, Screen};
use crate::db::run_catalog::{RunCatalog, RunCatalogSave};
use crate::ge;
use crate::http::{
    AppEvent,
    RecordingSavePending,
    RecordingSaved,
    RecordingStateStore,
    RecordingStatus,
    ReplaySaveStage,
    ReplaySaveStateStore,
    ReplaySaveStatus,
};
use crate::template_tokens::{RunTemplateTokens, format_iso_utc, format_time};

/// Internal safety margin added to both the pre- and post-run padding, on top of
/// the user's configured values and hidden from them, so a single-frame timing
/// window can't drop the level-start briefing or stats overlay (e.g. padding 0).
const MATCH_PADDING_BUFFER_SECS: f64 = 0.5;

/// A replay save taking this long is unusual, but OBS can still complete it.
/// Keep ownership of the request so a late identity-less event remains attached
/// to the correct run.
#[cfg(not(test))]
const REPLAY_SAVE_SLOW_WARNING: Duration = Duration::from_secs(20);
/// Avoid blocking all later saves forever if OBS never sends a completion event.
#[cfg(not(test))]
const REPLAY_SAVE_TIMEOUT: Duration = Duration::from_secs(120);
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
const OBS_OUTPUT_PATH_BUFFER_SIZE: usize = 4096;
static NEXT_REPLAY_TRACKING_ID: AtomicU64 = AtomicU64::new(1);

fn next_replay_tracking_id() -> u64 {
    NEXT_REPLAY_TRACKING_ID.fetch_add(1, Ordering::Relaxed)
}

/// Recording behaviour loaded when a monitor session starts. The saveable-clip
/// count is updated live; other options remain fixed for the session.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(rename_all = "camelCase")]
pub struct RecordingOptions {
    pub completed_output_path: String,
    pub recent_run_limit: usize,
    pub clip_filename_template: String,
    pub pre_run_padding_secs: f64,
    pub post_run_padding_secs: f64,
}

impl Default for RecordingOptions {
    fn default() -> Self {
        RecordingOptions {
            completed_output_path: String::new(),
            recent_run_limit: DEFAULT_RECENT_RUN_LIMIT,
            clip_filename_template: DEFAULT_CLIP_FILENAME_TEMPLATE.to_owned(),
            pre_run_padding_secs: DEFAULT_PRE_RUN_PADDING_SECS,
            post_run_padding_secs: DEFAULT_POST_RUN_PADDING_SECS,
        }
    }
}

impl RecordingOptions {
    fn non_negative_secs(value: f64, fallback: f64) -> f64 {
        if value.is_finite() { value.max(0.0) } else { fallback }
    }

    #[cfg_attr(test, allow(dead_code))]
    fn clip_filename_template(&self) -> &str {
        let trimmed = self.clip_filename_template.trim();
        if trimmed.is_empty() { DEFAULT_CLIP_FILENAME_TEMPLATE } else { trimmed }
    }

    fn pre_run_padding_secs(&self) -> f64 {
        Self::non_negative_secs(self.pre_run_padding_secs, 0.0) + MATCH_PADDING_BUFFER_SECS
    }

    fn post_run_padding_secs(&self) -> f64 {
        Self::non_negative_secs(self.post_run_padding_secs, DEFAULT_POST_RUN_PADDING_SECS) + MATCH_PADDING_BUFFER_SECS
    }

    fn save_delay(&self) -> Duration {
        Duration::from_secs_f64(self.post_run_padding_secs())
    }
}

include!("replay_buffer.rs");
include!("tracker.rs");
include!("save_pipeline.rs");
include!("clip_output.rs");

#[cfg(test)]
#[path = "tests.rs"]
mod recording_test;
