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

// ── YouTube Channel types ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChannelListResponse {
    pub kind: Option<String>,
    pub etag: Option<String>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    #[serde(rename = "prevPageToken")]
    pub prev_page_token: Option<String>,
    #[serde(rename = "pageInfo")]
    pub page_info: Option<PageInfo>,
    pub items: Option<Vec<Channel>>,
}

#[derive(Debug, Deserialize)]
pub struct PageInfo {
    #[serde(rename = "totalResults")]
    pub total_results: Option<u32>,
    #[serde(rename = "resultsPerPage")]
    pub results_per_page: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct Channel {
    pub kind: Option<String>,
    pub etag: Option<String>,
    pub id: Option<String>,
    pub snippet: Option<ChannelSnippet>,
    #[serde(rename = "contentDetails")]
    pub content_details: Option<ChannelContentDetails>,
    pub statistics: Option<ChannelStatistics>,
    #[serde(rename = "topicDetails")]
    pub topic_details: Option<ChannelTopicDetails>,
    pub status: Option<ChannelStatus>,
    #[serde(rename = "brandingSettings")]
    pub branding_settings: Option<ChannelBrandingSettings>,
    #[serde(rename = "auditDetails")]
    pub audit_details: Option<ChannelAuditDetails>,
    #[serde(rename = "contentOwnerDetails")]
    pub content_owner_details: Option<ChannelContentOwnerDetails>,
    pub localizations: Option<HashMap<String, LocalizedText>>,
}

#[derive(Debug, Deserialize)]
pub struct ChannelSnippet {
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "customUrl")]
    pub custom_url: Option<String>,
    #[serde(rename = "publishedAt")]
    pub published_at: Option<String>,
    pub thumbnails: Option<HashMap<String, Thumbnail>>,
    #[serde(rename = "defaultLanguage")]
    pub default_language: Option<String>,
    pub localized: Option<LocalizedText>,
    pub country: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LocalizedText {
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChannelContentDetails {
    #[serde(rename = "relatedPlaylists")]
    pub related_playlists: Option<RelatedPlaylists>,
}

#[derive(Debug, Deserialize)]
pub struct RelatedPlaylists {
    pub likes: Option<String>,
    pub favorites: Option<String>,
    pub uploads: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChannelStatistics {
    #[serde(rename = "viewCount")]
    pub view_count: Option<String>,
    #[serde(rename = "subscriberCount")]
    pub subscriber_count: Option<String>,
    #[serde(rename = "hiddenSubscriberCount")]
    pub hidden_subscriber_count: Option<bool>,
    #[serde(rename = "videoCount")]
    pub video_count: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChannelTopicDetails {
    #[serde(rename = "topicIds")]
    pub topic_ids: Option<Vec<String>>,
    #[serde(rename = "topicCategories")]
    pub topic_categories: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ChannelStatus {
    #[serde(rename = "privacyStatus")]
    pub privacy_status: Option<String>,
    #[serde(rename = "isLinked")]
    pub is_linked: Option<bool>,
    #[serde(rename = "longUploadsStatus")]
    pub long_uploads_status: Option<String>,
    #[serde(rename = "madeForKids")]
    pub made_for_kids: Option<bool>,
    #[serde(rename = "selfDeclaredMadeForKids")]
    pub self_declared_made_for_kids: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ChannelBrandingSettings {
    pub channel: Option<ChannelBrandingChannel>,
    pub watch: Option<ChannelBrandingWatch>,
}

#[derive(Debug, Deserialize)]
pub struct ChannelBrandingChannel {
    pub title: Option<String>,
    pub description: Option<String>,
    pub keywords: Option<String>,
    #[serde(rename = "trackingAnalyticsAccountId")]
    pub tracking_analytics_account_id: Option<String>,
    #[serde(rename = "unsubscribedTrailer")]
    pub unsubscribed_trailer: Option<String>,
    #[serde(rename = "defaultLanguage")]
    pub default_language: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChannelBrandingWatch {
    #[serde(rename = "textColor")]
    pub text_color: Option<String>,
    #[serde(rename = "backgroundColor")]
    pub background_color: Option<String>,
    #[serde(rename = "featuredPlaylistId")]
    pub featured_playlist_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChannelAuditDetails {
    #[serde(rename = "overallGoodStanding")]
    pub overall_good_standing: Option<bool>,
    #[serde(rename = "communityGuidelinesGoodStanding")]
    pub community_guidelines_good_standing: Option<bool>,
    #[serde(rename = "copyrightStrikesGoodStanding")]
    pub copyright_strikes_good_standing: Option<bool>,
    #[serde(rename = "contentIdClaimsGoodStanding")]
    pub content_id_claims_good_standing: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ChannelContentOwnerDetails {
    #[serde(rename = "contentOwner")]
    pub content_owner: Option<String>,
    #[serde(rename = "timeLinked")]
    pub time_linked: Option<String>,
}
