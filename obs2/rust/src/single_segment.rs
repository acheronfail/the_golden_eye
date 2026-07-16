use std::time::Instant;

use serde::{Deserialize, Serialize};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_real_time_secs: Option<f64>,
    pub splits: Vec<SingleSegmentSplit>,
}

impl SingleSegmentSnapshot {
    pub fn empty() -> Self {
        Self { category: None, started: false, total_real_time_secs: None, splits: Vec::new() }
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
    active: Option<ActiveSplit>,
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
            total_real_time_secs: None,
            splits,
        };
        snapshot.set_single_segment(state.clone());
        Ok(Some(Self { snapshot, state, run_started_at: None, active: None }))
    }

    pub fn on_frame(&mut self, now: Instant, m: &LevelMatch) {
        match m.screen {
            Screen::Start | Screen::Opts007 => self.start_split(now, m),
            Screen::Stats => self.complete_split(now, m),
            _ => {}
        }
    }

    fn start_split(&mut self, now: Instant, m: &LevelMatch) {
        let Some(index) = self.find_split(m) else {
            return;
        };
        if self.state.splits[index].status == SplitStatus::Complete || self.active.as_ref().is_some_and(|a| a.index == index) {
            return;
        }

        if self.run_started_at.is_none() {
            self.run_started_at = Some(now);
            self.state.started = true;
        }
        if let Some(active) = self.active.take()
            && self.state.splits[active.index].status == SplitStatus::Active
        {
            self.state.splits[active.index].status = SplitStatus::Pending;
        }
        self.state.splits[index].status = SplitStatus::Active;
        self.active = Some(ActiveSplit { index, started_at: now });
        self.publish(now);
    }

    fn complete_split(&mut self, now: Instant, m: &LevelMatch) {
        let Some(times) = m.times else {
            return;
        };
        let Some(index) = self.find_split(m) else {
            return;
        };
        if self.state.splits[index].status == SplitStatus::Complete {
            return;
        }

        if self.run_started_at.is_none() {
            self.run_started_at = Some(now);
            self.state.started = true;
        }
        let segment_started_at = self.active.as_ref().filter(|a| a.index == index).map(|a| a.started_at);
        let run_started_at = self.run_started_at.unwrap_or(now);
        let split = &mut self.state.splits[index];
        split.status = SplitStatus::Complete;
        split.real_time_secs = Some(now.saturating_duration_since(run_started_at).as_secs_f64());
        split.segment_real_time_secs = Some(now.saturating_duration_since(segment_started_at.unwrap_or(run_started_at)).as_secs_f64());
        split.game_time_secs = Some(times.time);
        if self.active.as_ref().is_some_and(|a| a.index == index) {
            self.active = None;
        }
        self.publish(now);
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
