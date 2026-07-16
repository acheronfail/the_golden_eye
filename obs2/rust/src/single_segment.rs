use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::ffmpeg;
use crate::http::{MonitorEvent, RecordingSaved};
use crate::recording::RecordingOptions;

use crate::cv::{LevelMatch, Screen};
use crate::ge;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunMode {
    Clips,
    AnyPercent,
    HundredPercent,
    All60,
}

impl Default for RunMode {
    fn default() -> Self {
        Self::Clips
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CategoryFile {
    categories: Vec<CategoryDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CategoryDefinition {
    id: RunMode,
    title: String,
    description: String,
    select_difficulty: bool,
    difficulties: Vec<String>,
    levels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SingleSegmentSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<SingleSegmentCategory>,
    pub started: bool,
    pub completed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_real_time_secs: Option<f64>,
    pub splits: Vec<SingleSegmentSplit>,
}

impl SingleSegmentSnapshot {
    pub fn empty() -> Self {
        Self { category: None, started: false, completed: false, total_real_time_secs: None, splits: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SingleSegmentCategory {
    pub id: RunMode,
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_difficulty: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SingleSegmentSplit {
    pub index: usize,
    pub level: String,
    pub level_number: i32,
    pub difficulty: String,
    pub difficulty_id: i32,
    pub status: SplitStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real_time_secs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_real_time_secs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_time_secs: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SplitStatus {
    Pending,
    Active,
    Complete,
}

#[derive(Debug, Clone)]
struct ActiveSplit {
    index: usize,
    started_at: Instant,
}

pub struct SingleSegmentTracker {
    snapshot: crate::http::SharedStateStore,
    state: SingleSegmentSnapshot,
    run_started_at: Option<Instant>,
    run_started_system: Option<SystemTime>,
    last_stats: Option<LevelMatch>,
    active: Option<ActiveSplit>,
    finalized: bool,
}

impl SingleSegmentTracker {
    pub fn new(snapshot: crate::http::SharedStateStore, mode: RunMode, difficulty: Option<i32>) -> anyhow::Result<Option<Self>> {
        if mode == RunMode::Clips {
            snapshot.set_single_segment(SingleSegmentSnapshot::empty());
            return Ok(None);
        }

        let category = category(mode)?;
        let selected_difficulty = if category.select_difficulty { Some(validated_difficulty(&category, difficulty)?) } else { None };
        let splits = build_splits(&category, selected_difficulty)?;
        let title = category.title.clone();
        let description = category.description.clone();
        let state = SingleSegmentSnapshot {
            category: Some(SingleSegmentCategory { id: mode, title, description, selected_difficulty }),
            started: false,
            completed: false,
            total_real_time_secs: None,
            splits,
        };
        snapshot.set_single_segment(state.clone());
        Ok(Some(Self {
            snapshot,
            state,
            run_started_at: None,
            run_started_system: None,
            last_stats: None,
            active: None,
            finalized: false,
        }))
    }

    pub fn on_frame(&mut self, now: Instant, m: &LevelMatch) -> Option<SingleSegmentFinish> {
        match m.screen {
            Screen::Start | Screen::Opts007 => self.start_split(now, m),
            Screen::Stats => self.complete_split(now, m),
            _ => None,
        }
    }

    fn start_split(&mut self, now: Instant, m: &LevelMatch) -> Option<SingleSegmentFinish> {
        let Some(index) = self.find_split(m) else {
            return None;
        };
        if self.state.splits[index].status == SplitStatus::Complete || self.active.as_ref().is_some_and(|a| a.index == index) {
            return None;
        }

        if self.run_started_at.is_none() {
            self.run_started_at = Some(now);
            self.run_started_system = Some(SystemTime::now());
            self.state.started = true;
            unsafe { crate::ffi::obs_frontend_recording_start() };
        }
        if let Some(active) = self.active.take()
            && self.state.splits[active.index].status == SplitStatus::Active
        {
            self.state.splits[active.index].status = SplitStatus::Pending;
        }
        self.state.splits[index].status = SplitStatus::Active;
        self.active = Some(ActiveSplit { index, started_at: now });
        self.publish(now);
        None
    }

    fn complete_split(&mut self, now: Instant, m: &LevelMatch) -> Option<SingleSegmentFinish> {
        let Some(times) = m.times else {
            return None;
        };
        let Some(index) = self.find_split(m) else {
            return None;
        };
        if self.state.splits[index].status == SplitStatus::Complete {
            return None;
        }

        if self.run_started_at.is_none() {
            self.run_started_at = Some(now);
            self.run_started_system = Some(SystemTime::now());
            self.state.started = true;
            unsafe { crate::ffi::obs_frontend_recording_start() };
        }
        let segment_started_at = self.active.as_ref().filter(|a| a.index == index).map(|a| a.started_at);
        let run_started_at = self.run_started_at.unwrap_or(now);
        let split = &mut self.state.splits[index];
        split.status = SplitStatus::Complete;
        split.real_time_secs = Some(now.saturating_duration_since(run_started_at).as_secs_f64());
        split.segment_real_time_secs = Some(now.saturating_duration_since(segment_started_at.unwrap_or(run_started_at)).as_secs_f64());
        split.game_time_secs = Some(times.time);
        self.last_stats = Some(m.clone());
        if self.active.as_ref().is_some_and(|a| a.index == index) {
            self.active = None;
        }
        self.publish(now);
        if index + 1 == self.state.splits.len() {
            self.finish(now, true)
        } else {
            None
        }
    }

    fn finish(&mut self, now: Instant, save: bool) -> Option<SingleSegmentFinish> {
        if self.finalized {
            return None;
        }
        let started_at = self.run_started_at?;
        self.finalized = true;
        self.state.completed = save;
        self.publish(now);
        let wait_generation = begin_recording_stop_wait();
        if unsafe { crate::ffi::obs_frontend_recording_active() } {
            unsafe { crate::ffi::obs_frontend_recording_stop() };
        }
        Some(SingleSegmentFinish {
            save,
            started_at,
            completed_at: now,
            started_system: self.run_started_system.unwrap_or_else(SystemTime::now),
            stats: self.last_stats.clone(),
            category: self.state.category.as_ref().map(|category| category.id).unwrap_or(RunMode::Clips),
            wait_generation,
        })
    }

    pub fn stop(&mut self, save: bool) -> Option<SingleSegmentFinish> {
        self.finish(Instant::now(), save)
    }

    fn find_split(&self, m: &LevelMatch) -> Option<usize> {
        let info = ge::level_info(m.mission, m.part)?;
        self.state.splits.iter().position(|split| split.level_number == info.number && split.difficulty_id == m.difficulty)
    }

    fn publish(&mut self, now: Instant) {
        self.state.total_real_time_secs = self.run_started_at.map(|start| now.saturating_duration_since(start).as_secs_f64());
        self.snapshot.set_single_segment(self.state.clone());
    }
}

#[derive(Debug, Clone)]
pub struct SingleSegmentFinish {
    save: bool,
    started_at: Instant,
    completed_at: Instant,
    started_system: SystemTime,
    stats: Option<LevelMatch>,
    category: RunMode,
    wait_generation: u64,
}


struct RecordingStopped {
    generation: u64,
    path: Option<String>,
}

static RECORDING_STOPPED: Mutex<RecordingStopped> = Mutex::new(RecordingStopped { generation: 0, path: None });
static RECORDING_STOPPED_CV: Condvar = Condvar::new();
const RECORDING_STOP_TIMEOUT: Duration = Duration::from_secs(30);

pub fn on_recording_stopped(path: Option<String>) {
    let mut stopped = RECORDING_STOPPED.lock().unwrap_or_else(|p| p.into_inner());
    stopped.generation = stopped.generation.wrapping_add(1);
    stopped.path = path;
    RECORDING_STOPPED_CV.notify_all();
}

fn begin_recording_stop_wait() -> u64 {
    RECORDING_STOPPED.lock().unwrap_or_else(|p| p.into_inner()).generation
}

fn wait_for_recording_stopped(since: u64) -> Option<String> {
    let stopped = RECORDING_STOPPED.lock().unwrap_or_else(|p| p.into_inner());
    let (mut stopped, _) = RECORDING_STOPPED_CV
        .wait_timeout_while(stopped, RECORDING_STOP_TIMEOUT, |state| state.generation == since)
        .unwrap_or_else(|p| p.into_inner());
    stopped.path.take()
}

pub fn finalize_recording(
    finish: SingleSegmentFinish,
    options: RecordingOptions,
    event_tx: broadcast::Sender<MonitorEvent>,
    source_name: String,
    rom_language: String,
) {
    std::thread::spawn(move || {
        let Some(recording_path) = wait_for_recording_stopped(finish.wait_generation) else {
            tracing::warn!("timed out waiting for OBS recording to stop");
            return;
        };
        if !finish.save {
            delete_file(&recording_path);
            return;
        }
        match trim_single_segment(&recording_path, &finish, &options, &source_name, &rom_language) {
            Ok(saved) => {
                delete_file(&recording_path);
                let _ = event_tx.send(MonitorEvent::RecordingSaved(saved));
            }
            Err(err) => tracing::error!(path = %recording_path, "failed to save single segment recording: {err:#}"),
        }
    });
}

fn trim_single_segment(
    recording_path: &str,
    finish: &SingleSegmentFinish,
    options: &RecordingOptions,
    source_name: &str,
    rom_language: &str,
) -> anyhow::Result<RecordingSaved> {
    let input = Path::new(recording_path);
    let duration = ffmpeg::duration_secs(input)?;
    let run_secs = finish.completed_at.saturating_duration_since(finish.started_at).as_secs_f64();
    let pre_padding = if options.pre_run_padding_secs.is_finite() { options.pre_run_padding_secs.max(0.0) } else { 0.0 };
    let start = (duration - run_secs - pre_padding).max(0.0);
    let end = duration;
    let dir = output_dir(input, options);
    ensure_output_directory(&dir)?;
    let ext = input.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    let stem = category_slug(finish.category);
    let output = unique_output_path(&dir.join(format!("{stem} - {}.{ext}", format_iso_local(finish.started_system))));
    let metadata = single_segment_metadata(finish, source_name, rom_language);
    ffmpeg::trim_with_metadata(input, &output, start, end, Some(&metadata))?;
    Ok(RecordingSaved {
        save_id: 1,
        path: output.to_string_lossy().into_owned(),
        replay_path: recording_path.to_owned(),
        duration_secs: end - start,
        failed: false,
        stats: finish.stats.clone(),
    })
}

fn single_segment_metadata(finish: &SingleSegmentFinish, source_name: &str, rom_language: &str) -> ffmpeg::ClipMetadata {
    ffmpeg::ClipMetadata {
        timestamp: format_iso_utc(finish.started_system),
        time: Some(format_time(finish.completed_at.saturating_duration_since(finish.started_at).as_secs() as i32)),
        time_seconds: Some(finish.completed_at.saturating_duration_since(finish.started_at).as_secs() as i32),
        level: category_title(finish.category).to_owned(),
        level_number: None,
        difficulty: None,
        status: "complete".to_owned(),
        rom_language: rom_language.to_owned(),
        source_name: source_name.to_owned(),
        comment: format!("Created by The Golden Eye OBS plugin v{}", crate::PLUGIN_VERSION),
        plugin_version: crate::PLUGIN_VERSION.to_owned(),
    }
}

fn output_dir(input: &Path, options: &RecordingOptions) -> PathBuf {
    configured_dir(&options.completed_output_path).unwrap_or_else(|| input.parent().unwrap_or_else(|| Path::new(".")).to_path_buf())
}

fn configured_dir(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed == "~" {
        crate::config::home_dir()
    } else if let Some(rest) = trimmed.strip_prefix("~/") {
        crate::config::home_dir().map(|home| home.join(rest))
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn category_slug(mode: RunMode) -> &'static str {
    match mode {
        RunMode::AnyPercent => "any-percent",
        RunMode::HundredPercent => "100-percent",
        RunMode::All60 => "all-60",
        RunMode::Clips => "single-segment",
    }
}

fn category_title(mode: RunMode) -> &'static str {
    match mode {
        RunMode::AnyPercent => "Single Segment Any%",
        RunMode::HundredPercent => "Single Segment 100%",
        RunMode::All60 => "Single Segment All 60",
        RunMode::Clips => "Single Segment",
    }
}

fn delete_file(path: &str) {
    if let Err(err) = std::fs::remove_file(path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(path, "failed to delete OBS recording: {err}");
    }
}

fn ensure_output_directory(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    Ok(())
}

fn unique_output_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("clip");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    for i in 2.. {
        let name = if ext.is_empty() { format!("{stem} ({i})") } else { format!("{stem} ({i}).{ext}") };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn format_time(seconds: i32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn format_iso_utc(time: SystemTime) -> String {
    chrono_like_iso(time, false)
}

fn format_iso_local(time: SystemTime) -> String {
    chrono_like_iso(time, true).replace(':', "-")
}

fn chrono_like_iso(time: SystemTime, local: bool) -> String {
    let secs = time.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (y, mo, d, h, mi, s) = utc_from_unix_seconds(secs);
    if local {
        format!("{y:04}-{mo:02}-{d:02} {h:02}-{mi:02}-{s:02}")
    } else {
        format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
    }
}

fn utc_from_unix_seconds(seconds: i64) -> (i64, i64, i64, i64, i64, i64) {
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    (y, m, d, rem / 3600, (rem % 3600) / 60, rem % 60)
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

fn category(mode: RunMode) -> anyhow::Result<CategoryDefinition> {
    let file: CategoryFile = serde_json::from_str(include_str!("../../single-segment-categories.json"))?;
    file.categories
        .into_iter()
        .find(|category| category.id == mode)
        .ok_or_else(|| anyhow::anyhow!("single segment category is not defined"))
}

fn validated_difficulty(category: &CategoryDefinition, difficulty: Option<i32>) -> anyhow::Result<i32> {
    let difficulty = difficulty.ok_or_else(|| anyhow::anyhow!("difficulty is required for this category"))?;
    let name = ge::difficulty_name(difficulty).ok_or_else(|| anyhow::anyhow!("unknown difficulty"))?;
    if category.difficulties.iter().any(|allowed| allowed == name) {
        Ok(difficulty)
    } else {
        Err(anyhow::anyhow!("difficulty is not allowed for this category"))
    }
}

fn build_splits(category: &CategoryDefinition, selected_difficulty: Option<i32>) -> anyhow::Result<Vec<SingleSegmentSplit>> {
    let difficulties = if let Some(difficulty) = selected_difficulty {
        vec![difficulty]
    } else {
        category.difficulties.iter().map(|name| difficulty_id(name)).collect::<anyhow::Result<Vec<_>>>()?
    };

    let mut splits = Vec::new();
    for level in &category.levels {
        let (level_number, level_name) = level_info_by_name(level)?;
        for difficulty_id in &difficulties {
            splits.push(SingleSegmentSplit {
                index: splits.len() + 1,
                level: level_name.to_owned(),
                level_number,
                difficulty: ge::difficulty_name(*difficulty_id).unwrap_or("Unknown").to_owned(),
                difficulty_id: *difficulty_id,
                status: SplitStatus::Pending,
                real_time_secs: None,
                segment_real_time_secs: None,
                game_time_secs: None,
            });
        }
    }
    Ok(splits)
}

fn level_info_by_name(name: &str) -> anyhow::Result<(i32, &'static str)> {
    for mission in 1..=9 {
        for part in 1..=5 {
            if let Some(info) = ge::level_info(mission, part)
                && info.name == name
            {
                return Ok((info.number, info.name));
            }
        }
    }
    Err(anyhow::anyhow!("unknown level in single segment category: {name}"))
}

fn difficulty_id(name: &str) -> anyhow::Result<i32> {
    [ge::AGENT, ge::SECRET_AGENT, ge::AGENT_00]
        .into_iter()
        .find(|difficulty| ge::difficulty_name(*difficulty) == Some(name))
        .ok_or_else(|| anyhow::anyhow!("unknown difficulty in single segment category: {name}"))
}
