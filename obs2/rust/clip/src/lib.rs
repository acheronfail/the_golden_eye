use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ts_rs::TS)]
#[ts(rename_all = "lowercase")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ClipMetadata {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub run_id: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub time_seconds: Option<i32>,
    pub level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub level_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub difficulty: Option<String>,
    #[ts(type = "string")]
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
    #[ts(type = "RunRetentionState")]
    pub retention_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub retention_reason: Option<String>,
}

fn default_retention_state() -> String {
    "kept".to_owned()
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
}
