use std::time::SystemTime;

use crate::cv::LevelMatch;
use crate::{ffmpeg, ge};

#[cfg(test)]
pub const SUPPORTED_TOKENS: &[&str] = &[
    "{obs_replay_name}",
    "{mission}",
    "{part}",
    "{levelNumber}",
    "{level}",
    "{time}",
    "{difficulty}",
    "{status}",
    "{timestamp}",
    "{timestamp_local}",
    "{plugin_version}",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunTemplateTokens {
    pub obs_replay_name: String,
    pub mission: String,
    pub part: String,
    pub level_number: String,
    pub level: String,
    pub time: String,
    pub difficulty: String,
    pub status: String,
    pub timestamp: String,
    pub timestamp_local: String,
    pub plugin_version: String,
}

impl RunTemplateTokens {
    pub fn from_match(stem: &str, status: &str, completed_at: SystemTime, stats: Option<&LevelMatch>) -> Self {
        let mission = stats
            .map(|m| if m.mission >= 0 { format!("{:02}", m.mission) } else { "??".to_owned() })
            .unwrap_or_default();
        let part = stats.map(|m| if m.part >= 0 { m.part.to_string() } else { "?".to_owned() }).unwrap_or_default();
        let difficulty = stats.and_then(|m| ge::difficulty_name(m.difficulty)).map(str::to_owned).unwrap_or_default();
        let level_info = stats.and_then(|m| ge::level_info(m.mission, m.part));
        let level = level_info.map(|info| info.name.to_owned()).unwrap_or_else(|| "unknown".to_owned());
        let level_number = level_info.map(|info| info.number.to_string()).unwrap_or_default();
        let time = stats.and_then(|m| m.times.map(|times| format_time(times.time))).unwrap_or_default();

        Self {
            obs_replay_name: stem.to_owned(),
            mission,
            part,
            level_number,
            level,
            time,
            difficulty,
            status: status.parse().expect("valid run status"),
            timestamp: format_iso_utc(completed_at),
            timestamp_local: format_iso_local(completed_at),
            plugin_version: crate::PLUGIN_VERSION.to_owned(),
        }
    }

    pub fn from_clip_metadata(stem: &str, metadata: &ffmpeg::ClipMetadata) -> Self {
        Self {
            obs_replay_name: stem.to_owned(),
            mission: String::new(),
            part: String::new(),
            level_number: metadata.level_number.map(|n| n.to_string()).unwrap_or_default(),
            level: metadata.level.clone(),
            time: metadata.time.clone().unwrap_or_default(),
            difficulty: metadata.difficulty.clone().unwrap_or_default(),
            status: metadata.status.as_str().to_owned(),
            timestamp_local: format_metadata_timestamp_local(&metadata.timestamp),
            timestamp: metadata.timestamp.clone(),
            plugin_version: crate::PLUGIN_VERSION.to_owned(),
        }
    }

    pub fn render(&self, template: &str) -> String {
        template
            .replace("{obs_replay_name}", &self.obs_replay_name)
            .replace("{mission}", &self.mission)
            .replace("{part}", &self.part)
            .replace("{difficulty}", &self.difficulty)
            .replace("{level}", &self.level)
            .replace("{levelNumber}", &self.level_number)
            .replace("{time}", &self.time)
            .replace("{status}", &self.status)
            .replace("{timestamp}", &self.timestamp)
            .replace("{timestamp_local}", &self.timestamp_local)
            .replace("{plugin_version}", &self.plugin_version)
    }
}

pub fn format_time(seconds: i32) -> String {
    let seconds = seconds.max(0);
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

pub fn system_time_unix_seconds(time: SystemTime) -> i64 {
    chrono::DateTime::<chrono::Utc>::from(time).timestamp()
}

pub fn format_iso_utc(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time).format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn format_iso_local(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(time).format("%Y-%m-%dT%H:%M:%S%z").to_string()
}

fn format_metadata_timestamp_local(timestamp: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .map(|time| time.with_timezone(&chrono::Local).format("%Y-%m-%dT%H:%M:%S%z").to_string())
        .unwrap_or_else(|_| timestamp.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::clip_metadata::RunStatus;

    #[test]
    fn render_leaves_unknown_tokens() {
        let tokens = RunTemplateTokens {
            obs_replay_name: "replay".to_owned(),
            mission: "01".to_owned(),
            part: "1".to_owned(),
            level_number: "1".to_owned(),
            level: "Dam".to_owned(),
            time: "01:23".to_owned(),
            difficulty: "Agent".to_owned(),
            status: "complete".to_owned(),
            timestamp: "2024-01-01T00:00:00Z".to_owned(),
            timestamp_local: "2024-01-01T00:00:00+0000".to_owned(),
            plugin_version: "1.2.3".to_owned(),
        };

        assert_eq!(tokens.render("{level} {not_a_token} {time}"), "Dam {not_a_token} 01:23");
    }

    #[test]
    fn metadata_tokens_convert_utc_timestamp_to_local() {
        let timestamp = "2026-07-18T10:30:45Z";
        let metadata = ffmpeg::ClipMetadata {
            run_id: "run-1".to_owned(),
            timestamp: timestamp.to_owned(),
            time: Some("01:23".to_owned()),
            time_seconds: Some(83),
            level: "Dam".to_owned(),
            level_number: Some(1),
            difficulty: Some("Agent".to_owned()),
            status: RunStatus::Complete,
            rom_language: "en".to_owned(),
            source_name: "N64 Capture".to_owned(),
            comment: "test".to_owned(),
            plugin_version: "test".to_owned(),
            retention_state: "kept".to_owned(),
            retention_reason: None,
        };

        let tokens = RunTemplateTokens::from_clip_metadata("replay", &metadata);
        let expected_local = chrono::DateTime::parse_from_rfc3339(timestamp)
            .unwrap()
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%dT%H:%M:%S%z")
            .to_string();

        assert_eq!(tokens.timestamp, timestamp);
        assert_eq!(tokens.timestamp_local, expected_local);
        assert_ne!(tokens.timestamp_local, timestamp);
    }

    #[test]
    fn metadata_tokens_keep_invalid_local_timestamp_fallback() {
        let timestamp = "not a timestamp";
        let metadata = ffmpeg::ClipMetadata {
            run_id: "run-2".to_owned(),
            timestamp: timestamp.to_owned(),
            time: None,
            time_seconds: None,
            level: "unknown".to_owned(),
            level_number: None,
            difficulty: None,
            status: RunStatus::Failed,
            rom_language: "en".to_owned(),
            source_name: "N64 Capture".to_owned(),
            comment: "test".to_owned(),
            plugin_version: "test".to_owned(),
            retention_state: "kept".to_owned(),
            retention_reason: None,
        };

        let tokens = RunTemplateTokens::from_clip_metadata("replay", &metadata);

        assert_eq!(tokens.timestamp, timestamp);
        assert_eq!(tokens.timestamp_local, timestamp);
    }

    #[test]
    fn supported_token_list_matches_renderer_names() {
        let template = SUPPORTED_TOKENS.join("|");
        let tokens = RunTemplateTokens {
            obs_replay_name: "replay".to_owned(),
            mission: "01".to_owned(),
            part: "1".to_owned(),
            level_number: "1".to_owned(),
            level: "Dam".to_owned(),
            time: "01:23".to_owned(),
            difficulty: "Agent".to_owned(),
            status: "complete".to_owned(),
            timestamp: "utc".to_owned(),
            timestamp_local: "local".to_owned(),
            plugin_version: "1.2.3".to_owned(),
        };

        let rendered = tokens.render(&template);
        for token in SUPPORTED_TOKENS {
            assert!(!rendered.contains(token), "{token} was not rendered");
        }
    }
}
