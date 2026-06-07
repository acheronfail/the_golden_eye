#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── OAuth / token types ───────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

/// Subset of what the Google token endpoint returns.
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

// ── Discord types ─────────────────────────────────────────────────────────────

/// The subset of a Discord message object we care about (returned when posting
/// a webhook with `wait=true`).
#[derive(Debug, Deserialize)]
pub struct DiscordMessage {
    pub id: String,
}

// ── YouTube LiveBroadcast types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LiveBroadcastResponse {
    pub items: Option<Vec<LiveBroadcastItem>>,
}

#[derive(Debug, Deserialize)]
pub struct LiveBroadcastItem {
    pub id: String,
    pub snippet: Option<LiveBroadcastSnippet>,
    pub status: Option<LiveBroadcastStatus>,
    #[serde(rename = "contentDetails")]
    pub content_details: Option<LiveBroadcastContentDetails>,
    pub statistics: Option<LiveBroadcastStatistics>,
    #[serde(rename = "monetizationDetails")]
    pub monetization_details: Option<LiveBroadcastMonetizationDetails>,
}

#[derive(Debug, Deserialize)]
pub struct LiveBroadcastSnippet {
    #[serde(rename = "publishedAt")]
    pub published_at: Option<String>,
    #[serde(rename = "channelId")]
    pub channel_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub thumbnails: Option<HashMap<String, Thumbnail>>,
    #[serde(rename = "scheduledStartTime")]
    pub scheduled_start_time: Option<String>,
    #[serde(rename = "scheduledEndTime")]
    pub scheduled_end_time: Option<String>,
    #[serde(rename = "actualStartTime")]
    pub actual_start_time: Option<String>,
    #[serde(rename = "actualEndTime")]
    pub actual_end_time: Option<String>,
    #[serde(rename = "isDefaultBroadcast")]
    pub is_default_broadcast: Option<bool>,
    #[serde(rename = "liveChatId")]
    pub live_chat_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Thumbnail {
    pub url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct LiveBroadcastStatus {
    #[serde(rename = "lifeCycleStatus")]
    pub life_cycle_status: Option<String>,
    #[serde(rename = "privacyStatus")]
    pub privacy_status: Option<String>,
    #[serde(rename = "recordingStatus")]
    pub recording_status: Option<String>,
    #[serde(rename = "madeForKids")]
    pub made_for_kids: Option<String>,
    #[serde(rename = "selfDeclaredMadeForKids")]
    pub self_declared_made_for_kids: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LiveBroadcastContentDetails {
    #[serde(rename = "boundStreamId")]
    pub bound_stream_id: Option<String>,
    #[serde(rename = "boundStreamLastUpdateTimeMs")]
    pub bound_stream_last_update_time_ms: Option<String>,
    #[serde(rename = "monitorStream")]
    pub monitor_stream: Option<MonitorStream>,
    #[serde(rename = "enableEmbed")]
    pub enable_embed: Option<bool>,
    #[serde(rename = "enableDvr")]
    pub enable_dvr: Option<bool>,
    #[serde(rename = "recordFromStart")]
    pub record_from_start: Option<bool>,
    #[serde(rename = "enableClosedCaptions")]
    pub enable_closed_captions: Option<bool>,
    #[serde(rename = "closedCaptionsType")]
    pub closed_captions_type: Option<String>,
    pub projection: Option<String>,
    #[serde(rename = "enableLowLatency")]
    pub enable_low_latency: Option<bool>,
    #[serde(rename = "latencyPreference")]
    pub latency_preference: Option<bool>,
    #[serde(rename = "enableAutoStart")]
    pub enable_auto_start: Option<bool>,
    #[serde(rename = "enableAutoStop")]
    pub enable_auto_stop: Option<bool>,
    #[serde(rename = "availabilityConfig")]
    pub availability_config: Option<AvailabilityConfig>,
}

#[derive(Debug, Deserialize)]
pub struct MonitorStream {
    #[serde(rename = "enableMonitorStream")]
    pub enable_monitor_stream: Option<bool>,
    #[serde(rename = "broadcastStreamDelayMs")]
    pub broadcast_stream_delay_ms: Option<u32>,
    #[serde(rename = "embedHtml")]
    pub embed_html: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AvailabilityConfig {
    #[serde(rename = "globalConfig")]
    pub global_config: Option<GlobalConfig>,
    #[serde(rename = "regionsConfig")]
    pub regions_config: Option<RegionsConfig>,
}

#[derive(Debug, Deserialize)]
pub struct GlobalConfig {
    #[serde(rename = "excludedRegionCodes")]
    pub excluded_region_codes: Option<Vec<String>>,
    pub interval: Option<TimeInterval>,
}

#[derive(Debug, Deserialize)]
pub struct RegionsConfig {
    #[serde(rename = "regionIntervals")]
    pub region_intervals: Option<Vec<RegionInterval>>,
}

#[derive(Debug, Deserialize)]
pub struct RegionInterval {
    #[serde(rename = "regionCode")]
    pub region_code: Option<String>,
    pub interval: Option<TimeInterval>,
}

#[derive(Debug, Deserialize)]
pub struct TimeInterval {
    #[serde(rename = "startTime")]
    pub start_time: Option<String>,
    #[serde(rename = "endTime")]
    pub end_time: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LiveBroadcastStatistics {
    #[serde(rename = "totalChatCount")]
    pub total_chat_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct LiveBroadcastMonetizationDetails {
    #[serde(rename = "adsMonetizationStatus")]
    pub ads_monetization_status: Option<String>,
    #[serde(rename = "eligibleForAdsMonetization")]
    pub eligible_for_ads_monetization: Option<bool>,
    #[serde(rename = "cuepointSchedule")]
    pub cuepoint_schedule: Option<CuepointSchedule>,
}

#[derive(Debug, Deserialize)]
pub struct CuepointSchedule {
    pub enabled: Option<bool>,
    #[serde(rename = "pauseAdsUntil")]
    pub pause_ads_until: Option<String>,
    #[serde(rename = "ytOptimizedCuepointConfig")]
    pub yt_optimized_cuepoint_config: Option<String>,
    #[serde(rename = "creatorCuepointConfig")]
    pub creator_cuepoint_config: Option<CreatorCuepointConfig>,
}

#[derive(Debug, Deserialize)]
pub struct CreatorCuepointConfig {
    #[serde(rename = "scheduleStrategy")]
    pub schedule_strategy: Option<String>,
    #[serde(rename = "repeatIntervalSecs")]
    pub repeat_interval_secs: Option<u32>,
}
