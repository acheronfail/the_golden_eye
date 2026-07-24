use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::db::statistics::{
    Bucket,
    MonitoringSessionDetail,
    MonitoringSessionSummary,
    StatisticsData,
    StatisticsQuery,
};
use crate::http::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsParams {
    from: Option<String>,
    to: Option<String>,
    bucket: Option<String>,
    level_number: Option<i32>,
    difficulty_number: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct SessionRangeParams {
    from: Option<String>,
    to: Option<String>,
}

pub async fn handle_statistics(
    State(state): State<AppState>,
    Query(params): Query<StatisticsParams>,
) -> Result<Json<StatisticsData>, (StatusCode, &'static str)> {
    let query = parse_statistics_params(params)?;
    let catalog = state.run_catalog.clone();
    tokio::task::spawn_blocking(move || catalog.statistics(query))
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "statistics task failed"))?
        .map(Json)
        .map_err(|err| {
            tracing::error!("failed to calculate statistics: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to load statistics")
        })
}

pub async fn handle_sessions(
    State(state): State<AppState>,
    Query(params): Query<SessionRangeParams>,
) -> Result<Json<Vec<MonitoringSessionSummary>>, (StatusCode, &'static str)> {
    let (from, to) = parse_range(params.from.as_deref(), params.to.as_deref())?;
    let catalog = state.run_catalog.clone();
    tokio::task::spawn_blocking(move || catalog.monitoring_sessions(from, to))
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "statistics task failed"))?
        .map(Json)
        .map_err(|err| {
            tracing::error!("failed to load monitoring sessions: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to load statistics")
        })
}

pub async fn handle_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<MonitoringSessionDetail>, (StatusCode, &'static str)> {
    let catalog = state.run_catalog.clone();
    tokio::task::spawn_blocking(move || catalog.monitoring_session(&session_id))
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "statistics task failed"))?
        .map_err(|err| {
            tracing::error!("failed to load monitoring session: {err:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to load statistics")
        })?
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "monitoring session not found"))
}

fn parse_statistics_params(params: StatisticsParams) -> Result<StatisticsQuery, (StatusCode, &'static str)> {
    let (from, to) = parse_range(params.from.as_deref(), params.to.as_deref())?;
    let bucket = match params.bucket.as_deref().unwrap_or("week") {
        "day" => Bucket::Day,
        "week" => Bucket::Week,
        "month" => Bucket::Month,
        "year" => Bucket::Year,
        _ => return Err((StatusCode::BAD_REQUEST, "invalid bucket")),
    };
    if params.level_number.is_some() != params.difficulty_number.is_some() {
        return Err((StatusCode::BAD_REQUEST, "levelNumber and difficultyNumber must be provided together"));
    }
    if params.level_number.is_some_and(|value| !(1..=20).contains(&value)) {
        return Err((StatusCode::BAD_REQUEST, "invalid levelNumber"));
    }
    if params.difficulty_number.is_some_and(|value| !(0..=3).contains(&value)) {
        return Err((StatusCode::BAD_REQUEST, "invalid difficultyNumber"));
    }
    Ok(StatisticsQuery {
        from,
        to,
        bucket,
        level_number: params.level_number,
        difficulty_number: params.difficulty_number,
    })
}

fn parse_range(from: Option<&str>, to: Option<&str>) -> Result<(Option<i64>, i64), (StatusCode, &'static str)> {
    let from = from.map(parse_instant).transpose()?;
    let to = to.map(parse_instant).transpose()?.unwrap_or_else(now_micros);
    if from.is_some_and(|from| from >= to) {
        return Err((StatusCode::BAD_REQUEST, "from must be before to"));
    }
    Ok((from, to))
}

fn parse_instant(value: &str) -> Result<i64, (StatusCode, &'static str)> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.timestamp_micros())
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid RFC 3339 date"))
}

fn now_micros() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_micros().min(i64::MAX as u128) as i64,
        Err(err) => -(err.duration().as_micros().min(i64::MAX as u128) as i64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_filters() {
        let parsed = parse_statistics_params(StatisticsParams {
            from: Some("2026-07-01T00:00:00+10:00".to_owned()),
            to: Some("2026-08-01T00:00:00+10:00".to_owned()),
            bucket: Some("year".to_owned()),
            level_number: Some(7),
            difficulty_number: Some(2),
        })
        .unwrap();
        assert_eq!(parsed.bucket, Bucket::Year);
        assert_eq!(parsed.level_number, Some(7));
        assert_eq!(parsed.difficulty_number, Some(2));
    }

    #[test]
    fn rejects_invalid_or_partial_cohorts() {
        let result = parse_statistics_params(StatisticsParams {
            from: None,
            to: None,
            bucket: Some("quarter".to_owned()),
            level_number: None,
            difficulty_number: None,
        });
        assert_eq!(result.unwrap_err().0, StatusCode::BAD_REQUEST);

        let result = parse_statistics_params(StatisticsParams {
            from: None,
            to: None,
            bucket: None,
            level_number: Some(1),
            difficulty_number: None,
        });
        assert_eq!(result.unwrap_err().0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn rejects_reversed_ranges() {
        let result = parse_range(Some("2026-08-01T00:00:00Z"), Some("2026-07-01T00:00:00Z"));
        assert_eq!(result.unwrap_err().0, StatusCode::BAD_REQUEST);
    }
}
