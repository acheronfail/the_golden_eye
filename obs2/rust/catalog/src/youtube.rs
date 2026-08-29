use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "export", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export", ts(rename = "YouTubeUploadHistoryEntry", rename_all = "camelCase"))]
pub struct UploadHistoryEntry {
    pub path: String,
    #[serde(flatten)]
    pub youtube: YoutubeMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "export", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export", ts(rename = "RunYouTubeVideo", rename_all = "camelCase"))]
pub struct YoutubeMetadata {
    pub video_id: String,
    pub video_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "export", ts(optional))]
    pub uploaded_at: Option<String>,
    pub title: String,
    #[serde(default)]
    pub source: YoutubeAssociationSource,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "export", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export", ts(rename = "YouTubeAssociationSource", rename_all = "camelCase"))]
pub enum YoutubeAssociationSource {
    #[default]
    PluginUpload,
    ManualLink,
    TheElite,
}
