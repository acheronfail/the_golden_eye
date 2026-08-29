use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadHistoryEntry {
    pub path: String,
    #[serde(flatten)]
    pub youtube: YoutubeMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YoutubeMetadata {
    pub video_id: String,
    pub video_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uploaded_at: Option<String>,
    pub title: String,
    #[serde(default)]
    pub source: YoutubeAssociationSource,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum YoutubeAssociationSource {
    #[default]
    PluginUpload,
    ManualLink,
    TheElite,
}
