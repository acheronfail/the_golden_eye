use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use serde::Serialize;

use super::runs::{CombinedBestRow, MonitorSessionRow, RunStatisticFact};
use crate::models::clip_metadata::RunStatus;

pub const COMBINED_FALLBACK_SECONDS: i32 = 1023;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, Copy)]
pub struct StatisticsQuery {
    pub from: Option<i64>,
    pub to: i64,
    pub bucket: Bucket,
    pub level_number: Option<i32>,
    pub difficulty_number: Option<i32>,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusCounts {
    pub total: usize,
    pub complete: usize,
    pub failed: usize,
    pub abort: usize,
    pub kia: usize,
}

impl StatusCounts {
    fn add(&mut self, status: RunStatus) {
        self.total += 1;
        match status {
            RunStatus::Complete => self.complete += 1,
            RunStatus::Failed => self.failed += 1,
            RunStatus::Abort => self.abort += 1,
            RunStatus::Kia => self.kia += 1,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsData {
    pub range: StatisticsRange,
    pub summary: StatisticsSummary,
    pub by_level: Vec<LevelCounts>,
    pub overall_buckets: Vec<BucketCounts>,
    pub selected_cohort: Option<SelectedCohort>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsRange {
    pub from: Option<String>,
    pub to: String,
    pub bucket: &'static str,
    pub time_zone: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsSummary {
    pub counts: StatusCounts,
    pub total_session_seconds: f64,
    pub combined_best_times: CombinedBestTimes,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CombinedBestTimes {
    pub overall_seconds: i32,
    pub recorded_cells: usize,
    pub total_cells: usize,
    pub by_difficulty: Vec<DifficultyCombinedBest>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DifficultyCombinedBest {
    pub difficulty_number: i32,
    pub total_seconds: i32,
    pub recorded_levels: usize,
    pub total_levels: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelCounts {
    pub level_number: Option<i32>,
    pub counts: StatusCounts,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketCounts {
    pub start: String,
    pub end: String,
    pub counts: StatusCounts,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedCohort {
    pub level_number: i32,
    pub difficulty_number: i32,
    pub counts: StatusCounts,
    pub buckets: Vec<BucketCounts>,
    pub run_times: Vec<RunTimePoint>,
    pub untimed_runs: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunTimePoint {
    pub run_id: String,
    pub completed_at: String,
    pub status: RunStatus,
    pub time_seconds: i32,
}

pub fn aggregate(
    facts: Vec<RunStatisticFact>,
    combined_rows: Vec<CombinedBestRow>,
    total_session_seconds: f64,
    query: StatisticsQuery,
) -> StatisticsData {
    let mut counts = StatusCounts::default();
    let mut by_level = BTreeMap::<Option<i32>, StatusCounts>::new();
    for fact in &facts {
        counts.add(fact.status);
        by_level.entry(fact.level_number).or_default().add(fact.status);
    }

    let overall_buckets = bucket_counts(&facts, query.from, query.to, query.bucket);
    let selected_dimensions = match (query.level_number, query.difficulty_number) {
        (Some(level), Some(difficulty)) => Some((level, difficulty)),
        _ => {
            let mut cohorts = BTreeMap::<(i32, i32), usize>::new();
            for fact in &facts {
                if let (Some(level), Some(difficulty)) = (fact.level_number, fact.difficulty_number) {
                    *cohorts.entry((level, difficulty)).or_default() += 1;
                }
            }
            cohorts.into_iter().max_by_key(|(_, count)| *count).map(|(cohort, _)| cohort)
        }
    };
    let selected_cohort = match selected_dimensions {
        Some((level_number, difficulty_number)) => {
            let cohort = facts
                .iter()
                .filter(|fact| {
                    fact.level_number == Some(level_number) && fact.difficulty_number == Some(difficulty_number)
                })
                .cloned()
                .collect::<Vec<_>>();
            let mut cohort_counts = StatusCounts::default();
            let mut run_times = Vec::new();
            for fact in &cohort {
                cohort_counts.add(fact.status);
                if let Some(time_seconds) = fact.time_seconds {
                    run_times.push(RunTimePoint {
                        run_id: fact.run_id.clone(),
                        completed_at: format_micros(fact.completed_unix_micros),
                        status: fact.status,
                        time_seconds,
                    });
                }
            }
            Some(SelectedCohort {
                level_number,
                difficulty_number,
                counts: cohort_counts,
                buckets: bucket_counts(&cohort, query.from, query.to, query.bucket),
                untimed_runs: cohort_counts.total.saturating_sub(run_times.len()),
                run_times,
            })
        }
        None => None,
    };

    StatisticsData {
        range: StatisticsRange {
            from: query.from.map(format_micros),
            to: format_micros(query.to),
            bucket: match query.bucket {
                Bucket::Day => "day",
                Bucket::Week => "week",
                Bucket::Month => "month",
            },
            time_zone: Local::now().format("%Z").to_string(),
        },
        summary: StatisticsSummary {
            counts,
            total_session_seconds,
            combined_best_times: combined_best_times(combined_rows),
        },
        by_level: by_level.into_iter().map(|(level_number, counts)| LevelCounts { level_number, counts }).collect(),
        overall_buckets,
        selected_cohort,
    }
}

fn combined_best_times(rows: Vec<CombinedBestRow>) -> CombinedBestTimes {
    let mut cells = [[COMBINED_FALLBACK_SECONDS; 20]; 3];
    let mut recorded = [[false; 20]; 3];
    for row in rows {
        let Ok(difficulty) = usize::try_from(row.difficulty_number) else {
            continue;
        };
        let Ok(level) = usize::try_from(row.level_number - 1) else {
            continue;
        };
        if difficulty < 3 && level < 20 {
            cells[difficulty][level] = row.time_seconds;
            recorded[difficulty][level] = true;
        }
    }
    let by_difficulty = (0..3)
        .map(|difficulty| DifficultyCombinedBest {
            difficulty_number: difficulty as i32,
            total_seconds: cells[difficulty].iter().sum(),
            recorded_levels: recorded[difficulty].iter().filter(|value| **value).count(),
            total_levels: 20,
        })
        .collect::<Vec<_>>();
    CombinedBestTimes {
        overall_seconds: by_difficulty.iter().map(|value| value.total_seconds).sum(),
        recorded_cells: by_difficulty.iter().map(|value| value.recorded_levels).sum(),
        total_cells: 60,
        by_difficulty,
    }
}

fn bucket_counts(facts: &[RunStatisticFact], from: Option<i64>, to: i64, bucket: Bucket) -> Vec<BucketCounts> {
    let Some(first) = from.or_else(|| facts.first().map(|fact| fact.completed_unix_micros)) else {
        return Vec::new();
    };
    if first >= to {
        return Vec::new();
    }
    let mut grouped = BTreeMap::<i64, StatusCounts>::new();
    for fact in facts {
        let start = bucket_start(local_datetime(fact.completed_unix_micros), bucket);
        grouped.entry(start.timestamp_micros()).or_default().add(fact.status);
    }

    let mut current = bucket_start(local_datetime(first), bucket);
    let end = local_datetime(to);
    let mut result = Vec::new();
    while current < end {
        let next = next_bucket(current, bucket);
        result.push(BucketCounts {
            start: current.to_rfc3339(),
            end: next.to_rfc3339(),
            counts: grouped.remove(&current.timestamp_micros()).unwrap_or_default(),
        });
        current = next;
    }
    result
}

fn bucket_start(value: DateTime<Local>, bucket: Bucket) -> DateTime<Local> {
    let date = match bucket {
        Bucket::Day => value.date_naive(),
        Bucket::Week => value.date_naive() - Duration::days(i64::from(value.weekday().num_days_from_monday())),
        Bucket::Month => NaiveDate::from_ymd_opt(value.year(), value.month(), 1).expect("valid local month"),
    };
    local_midnight(date)
}

fn next_bucket(value: DateTime<Local>, bucket: Bucket) -> DateTime<Local> {
    match bucket {
        Bucket::Day => local_midnight(value.date_naive() + Duration::days(1)),
        Bucket::Week => local_midnight(value.date_naive() + Duration::days(7)),
        Bucket::Month => {
            let (year, month) =
                if value.month() == 12 { (value.year() + 1, 1) } else { (value.year(), value.month() + 1) };
            local_midnight(NaiveDate::from_ymd_opt(year, month, 1).expect("valid next month"))
        }
    }
}

fn local_midnight(date: NaiveDate) -> DateTime<Local> {
    let naive = date.and_hms_opt(0, 0, 0).expect("valid midnight");
    Local.from_local_datetime(&naive).earliest().unwrap_or_else(|| Local.from_utc_datetime(&naive))
}

fn local_datetime(micros: i64) -> DateTime<Local> {
    DateTime::<Utc>::from_timestamp_micros(micros).unwrap_or(DateTime::<Utc>::UNIX_EPOCH).with_timezone(&Local)
}

pub fn format_micros(micros: i64) -> String {
    local_datetime(micros).to_rfc3339()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringSessionSummary {
    pub session_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub source_name: String,
    pub initial_cv_language: Option<String>,
    pub plugin_version: String,
    pub end_reason: Option<String>,
    pub counts: StatusCounts,
    pub distinct_levels: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringSessionDetail {
    #[serde(flatten)]
    pub summary: MonitoringSessionSummary,
    pub attempts: Vec<SessionAttempt>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAttempt {
    pub run_id: String,
    pub completed_at: String,
    pub elapsed_seconds: f64,
    pub level_number: Option<i32>,
    pub difficulty_number: Option<i32>,
    pub status: RunStatus,
    pub time_seconds: Option<i32>,
}

pub fn session_detail(session: MonitorSessionRow, facts: Vec<RunStatisticFact>) -> MonitoringSessionDetail {
    let mut counts = StatusCounts::default();
    let mut distinct = BTreeMap::<Option<i32>, ()>::new();
    let attempts = facts
        .iter()
        .map(|fact| {
            counts.add(fact.status);
            distinct.insert(fact.level_number, ());
            SessionAttempt {
                run_id: fact.run_id.clone(),
                completed_at: format_micros(fact.completed_unix_micros),
                elapsed_seconds: (fact.completed_unix_micros - session.started_unix_micros) as f64 / 1_000_000.0,
                level_number: fact.level_number,
                difficulty_number: fact.difficulty_number,
                status: fact.status,
                time_seconds: fact.time_seconds,
            }
        })
        .collect();
    MonitoringSessionDetail {
        summary: MonitoringSessionSummary {
            session_id: session.session_id,
            started_at: format_micros(session.started_unix_micros),
            ended_at: session.ended_unix_micros.map(format_micros),
            source_name: session.source_name,
            initial_cv_language: session.initial_cv_language,
            plugin_version: session.plugin_version,
            end_reason: session.end_reason,
            counts,
            distinct_levels: distinct.keys().filter(|level| level.is_some()).count(),
        },
        attempts,
    }
}
