use std::str::FromStr;

use ffmpeg_next::{Dictionary, DictionaryRef};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const TAG_CREATED_BY: &str = "fail.acheron.thegoldeneye.created_by";
const TAG_CREATED_BY_VALUE: &str = "the-golden-eye";
const TAG_SCHEMA_VERSION: &str = "fail.acheron.thegoldeneye.schema_version";
const TAG_SCHEMA_VERSION_VALUE: &str = "2";
const TAG_PLUGIN_VERSION: &str = "fail.acheron.thegoldeneye.plugin_version";
const TAG_RUN_TIMESTAMP: &str = "fail.acheron.thegoldeneye.run_timestamp";
const TAG_RUN_TIME: &str = "fail.acheron.thegoldeneye.time";
const TAG_RUN_TIME_SECONDS: &str = "fail.acheron.thegoldeneye.time_seconds";
const TAG_LEVEL: &str = "fail.acheron.thegoldeneye.level";
const TAG_LEVEL_NUMBER: &str = "fail.acheron.thegoldeneye.level_number";
const TAG_DIFFICULTY: &str = "fail.acheron.thegoldeneye.difficulty";
const TAG_STATUS: &str = "fail.acheron.thegoldeneye.status";
const TAG_WAS_PERSONAL_BEST: &str = "fail.acheron.thegoldeneye.was_personal_best";
const TAG_GAME_LANGUAGE: &str = "fail.acheron.thegoldeneye.game_language";
const TAG_LEGACY_ROM_LANGUAGE: &str = "fail.acheron.thegoldeneye.rom_language";
const TAG_ROM_VERSION: &str = "fail.acheron.thegoldeneye.rom_version";
const TAG_SOURCE_NAME: &str = "fail.acheron.thegoldeneye.source_name";
const TAG_RUN_ID: &str = "fail.acheron.thegoldeneye.run_id";
const TAG_RETENTION_STATE: &str = "fail.acheron.thegoldeneye.retention_state";
const TAG_RETENTION_REASON: &str = "fail.acheron.thegoldeneye.retention_reason";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunStatus {
    Complete,
    Failed,
    Abort,
    Kia,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Complete => "complete",
            RunStatus::Failed => "failed",
            RunStatus::Abort => "abort",
            RunStatus::Kia => "kia",
        }
    }

    pub fn is_failed(self) -> bool {
        matches!(self, RunStatus::Failed | RunStatus::Abort | RunStatus::Kia)
    }
}

impl FromStr for RunStatus {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "complete" | "completed" => Ok(RunStatus::Complete),
            "failed" => Ok(RunStatus::Failed),
            "abort" => Ok(RunStatus::Abort),
            "kia" => Ok(RunStatus::Kia),
            _ => Err(()),
        }
    }
}

impl Serialize for RunStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RunStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        RunStatus::from_str(&value).map_err(|_| serde::de::Error::custom(format!("unknown run status {value}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RomVersion {
    #[serde(rename = "ntsc-u")]
    NtscU,
    #[serde(rename = "ntsc-j")]
    NtscJ,
    #[serde(rename = "pal")]
    Pal,
}

impl RomVersion {
    pub fn as_str(self) -> &'static str {
        match self {
            RomVersion::NtscU => "NTSC-U",
            RomVersion::NtscJ => "NTSC-J",
            RomVersion::Pal => "PAL",
        }
    }

    pub fn game_language(self) -> &'static str {
        match self {
            RomVersion::NtscJ => "jp",
            RomVersion::NtscU | RomVersion::Pal => "en",
        }
    }
}

impl FromStr for RomVersion {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ntsc-u" => Ok(RomVersion::NtscU),
            "ntsc-j" => Ok(RomVersion::NtscJ),
            "pal" => Ok(RomVersion::Pal),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipMetadata {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub run_id: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_seconds: Option<i32>,
    pub level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<String>,
    pub status: RunStatus,
    #[serde(default)]
    pub was_personal_best: bool,
    #[serde(alias = "romLanguage")]
    pub game_language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rom_version: Option<RomVersion>,
    pub source_name: String,
    pub comment: String,
    pub plugin_version: String,
    #[serde(default = "default_retention_state")]
    pub retention_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_reason: Option<String>,
}

fn default_retention_state() -> String {
    "kept".to_owned()
}

pub fn is_ffmpeg_plugin_tag(key: &str) -> bool {
    [
        TAG_CREATED_BY,
        TAG_SCHEMA_VERSION,
        TAG_PLUGIN_VERSION,
        TAG_RUN_TIMESTAMP,
        TAG_RUN_TIME,
        TAG_RUN_TIME_SECONDS,
        TAG_LEVEL,
        TAG_LEVEL_NUMBER,
        TAG_DIFFICULTY,
        TAG_STATUS,
        TAG_WAS_PERSONAL_BEST,
        TAG_GAME_LANGUAGE,
        TAG_LEGACY_ROM_LANGUAGE,
        TAG_ROM_VERSION,
        TAG_SOURCE_NAME,
        TAG_RUN_ID,
        TAG_RETENTION_STATE,
        TAG_RETENTION_REASON,
        "comment",
    ]
    .iter()
    .any(|candidate| key.eq_ignore_ascii_case(candidate))
}

impl ClipMetadata {
    pub fn write_ffmpeg_tags(&self, metadata: &mut Dictionary) {
        metadata.set(TAG_CREATED_BY, TAG_CREATED_BY_VALUE);
        metadata.set(TAG_SCHEMA_VERSION, TAG_SCHEMA_VERSION_VALUE);
        metadata.set(TAG_PLUGIN_VERSION, &clean_metadata_value(&self.plugin_version));
        metadata.set(TAG_RUN_TIMESTAMP, &clean_metadata_value(&self.timestamp));
        set_optional_metadata(metadata, TAG_RUN_TIME, self.time.as_deref());
        set_optional_metadata(metadata, TAG_RUN_TIME_SECONDS, self.time_seconds.map(|s| s.to_string()).as_deref());
        metadata.set(TAG_LEVEL, &clean_metadata_value(&self.level));
        set_optional_metadata(metadata, TAG_LEVEL_NUMBER, self.level_number.map(|n| n.to_string()).as_deref());
        set_optional_metadata(metadata, TAG_DIFFICULTY, self.difficulty.as_deref());
        metadata.set(TAG_STATUS, self.status.as_str());
        metadata.set(TAG_WAS_PERSONAL_BEST, if self.was_personal_best { "true" } else { "false" });
        metadata.set(TAG_GAME_LANGUAGE, &clean_metadata_value(&self.game_language));
        set_optional_metadata(metadata, TAG_ROM_VERSION, self.rom_version.map(RomVersion::as_str));
        metadata.set(TAG_SOURCE_NAME, &clean_metadata_value(&self.source_name));
        if !self.run_id.is_empty() {
            metadata.set(TAG_RUN_ID, &clean_metadata_value(&self.run_id));
        }
        metadata.set(TAG_RETENTION_STATE, &clean_metadata_value(&self.retention_state));
        set_optional_metadata(metadata, TAG_RETENTION_REASON, self.retention_reason.as_deref());
        metadata.set("comment", &clean_metadata_value(&self.comment));
    }

    pub fn from_ffmpeg_tags(metadata: &DictionaryRef<'_>) -> Option<Self> {
        let created_by = get_metadata(metadata, TAG_CREATED_BY)?;
        if created_by != TAG_CREATED_BY_VALUE {
            return None;
        }

        let timestamp = get_metadata(metadata, TAG_RUN_TIMESTAMP)?;
        let status = get_metadata(metadata, TAG_STATUS).and_then(|value| RunStatus::from_str(&value).ok())?;
        let was_personal_best =
            get_metadata(metadata, TAG_WAS_PERSONAL_BEST).is_some_and(|value| matches!(value.as_str(), "true" | "1"));
        let level = get_metadata(metadata, TAG_LEVEL).unwrap_or_else(|| "unknown".to_owned());
        let comment = get_metadata(metadata, "comment").unwrap_or_default();
        let plugin_version = get_metadata(metadata, TAG_PLUGIN_VERSION).unwrap_or_default();
        let time = get_metadata(metadata, TAG_RUN_TIME);
        let time_seconds = get_metadata(metadata, TAG_RUN_TIME_SECONDS).and_then(|value| value.parse::<i32>().ok());
        let level_number = get_metadata(metadata, TAG_LEVEL_NUMBER).and_then(|value| value.parse::<i32>().ok());
        let difficulty = get_metadata(metadata, TAG_DIFFICULTY);
        let game_language = get_metadata(metadata, TAG_GAME_LANGUAGE)
            .or_else(|| get_metadata(metadata, TAG_LEGACY_ROM_LANGUAGE))
            .unwrap_or_default();
        let rom_version = get_metadata(metadata, TAG_ROM_VERSION).and_then(|value| RomVersion::from_str(&value).ok());
        let source_name = get_metadata(metadata, TAG_SOURCE_NAME).unwrap_or_default();
        let run_id = get_metadata(metadata, TAG_RUN_ID).unwrap_or_default();
        let retention_state = get_metadata(metadata, TAG_RETENTION_STATE).unwrap_or_else(default_retention_state);
        let retention_reason = get_metadata(metadata, TAG_RETENTION_REASON);

        Some(Self {
            run_id,
            timestamp,
            time,
            time_seconds,
            level,
            level_number,
            difficulty,
            status,
            was_personal_best,
            game_language,
            rom_version,
            source_name,
            comment,
            plugin_version,
            retention_state,
            retention_reason,
        })
    }
}

fn clean_metadata_value(value: &str) -> String {
    value.replace('\0', " ").trim().to_owned()
}

fn set_optional_metadata(metadata: &mut Dictionary, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        let cleaned = clean_metadata_value(value);
        if !cleaned.is_empty() {
            metadata.set(key, &cleaned);
        }
    }
}

fn get_metadata(metadata: &DictionaryRef<'_>, key: &str) -> Option<String> {
    metadata
        .get(key)
        .or_else(|| metadata.iter().find(|(candidate, _)| candidate.eq_ignore_ascii_case(key)).map(|(_, value)| value))
        .map(str::to_owned)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_legacy_catalog_language_and_defaults_rom_version() {
        let metadata: ClipMetadata = serde_json::from_value(serde_json::json!({
            "timestamp": "2026-07-24T12:00:00Z",
            "time": "00:33",
            "level": "Frigate",
            "status": "complete",
            "romLanguage": "jp",
            "sourceName": "Legacy catalog",
            "comment": "",
            "pluginVersion": "1.0.0"
        }))
        .unwrap();

        assert_eq!(metadata.game_language, "jp");
        assert_eq!(metadata.rom_version, None);
        assert!(!metadata.was_personal_best);
    }

    #[test]
    fn reads_legacy_clip_language_tag() {
        let mut tags = Dictionary::new();
        tags.set(TAG_CREATED_BY, TAG_CREATED_BY_VALUE);
        tags.set(TAG_RUN_TIMESTAMP, "2026-07-24T12:00:00Z");
        tags.set(TAG_STATUS, "complete");
        tags.set(TAG_LEVEL, "Frigate");
        tags.set(TAG_LEGACY_ROM_LANGUAGE, "jp");

        let metadata = ClipMetadata::from_ffmpeg_tags(&tags).unwrap();
        assert_eq!(metadata.game_language, "jp");
        assert_eq!(metadata.rom_version, None);
        assert!(!metadata.was_personal_best);
    }
}
