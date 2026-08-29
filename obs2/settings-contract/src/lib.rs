use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(feature = "export")]
use ts_rs::TS;

pub const DEFAULT_CLIP_FILENAME_TEMPLATE: &str =
    "{level} - {difficulty} - {time} - {timestamp_local}";
pub const DEFAULT_PRE_RUN_PADDING_SECS: f64 = 5.0;
pub const DEFAULT_POST_RUN_PADDING_SECS: f64 = 5.0;
pub const DEFAULT_RECENT_RUN_LIMIT: usize = 10;
pub const MAX_RECENT_RUN_LIMIT: usize = 20;
pub const DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE: &str =
    "🟢 Bond is now streaming at: {broadcast_url}";
pub const DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE: &str =
    "🔴 Bond stopped streaming at <t:{unix_seconds}:F>: {broadcast_url}";
pub const DEFAULT_YOUTUBE_TITLE_TEMPLATE: &str = "{level} - {difficulty} - {time}";
pub const DEFAULT_YOUTUBE_DESCRIPTION_TEMPLATE: &str =
    "Achieved at {datetime_local}\n\nRecorded with The Golden Eye {plugin_version}.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "export", derive(TS))]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "export", ts(rename_all = "kebab-case"))]
pub enum MonitorDesign {
    SignalBand,
    MissionGlass,
    Debug,
}

pub const DEFAULT_MONITOR_DESIGN: MonitorDesign = MonitorDesign::SignalBand;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "export", derive(TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export", ts(rename_all = "camelCase"))]
pub enum UpdateCheckInterval {
    Monthly,
    Weekly,
    Daily,
    Never,
}

pub const DEFAULT_UPDATE_CHECK_INTERVAL: UpdateCheckInterval = UpdateCheckInterval::Weekly;

impl UpdateCheckInterval {
    pub fn interval_secs(self) -> Option<u64> {
        match self {
            UpdateCheckInterval::Daily => Some(24 * 60 * 60),
            UpdateCheckInterval::Weekly => Some(7 * 24 * 60 * 60),
            UpdateCheckInterval::Monthly => Some(30 * 24 * 60 * 60),
            UpdateCheckInterval::Never => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "export", derive(TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "export", ts(rename_all = "camelCase"))]
pub enum YoutubeVisibility {
    Public,
    Unlisted,
    Private,
}

impl YoutubeVisibility {
    pub fn as_youtube_str(self) -> &'static str {
        match self {
            YoutubeVisibility::Public => "public",
            YoutubeVisibility::Unlisted => "unlisted",
            YoutubeVisibility::Private => "private",
        }
    }
}

pub const DEFAULT_YOUTUBE_VISIBILITY: YoutubeVisibility = YoutubeVisibility::Unlisted;

/// Persisted settings shared by the Rust runtime and bundled browser app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "export", derive(TS))]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "export", ts(rename_all = "camelCase"))]
pub struct AppSettings {
    pub stop_replay_buffer_when_monitor_stopped: bool,
    pub stop_replay_buffer_prompt_shown: bool,
    pub monitor_design: MonitorDesign,
    pub show_monitor_fps: bool,
    pub show_developer_settings: bool,
    pub show_source_previews: bool,
    pub last_used_source_name: Option<String>,
    pub welcome_modal_shown: bool,
    pub completed_output_path: String,
    pub recent_run_limit: usize,
    pub clip_filename_template: String,
    pub pre_run_padding_secs: f64,
    pub post_run_padding_secs: f64,
    pub discord_notifications_enabled: bool,
    pub discord_webhook_url: String,
    pub streaming_started_message_template: String,
    pub streaming_stopped_message_template: String,
    pub update_check_interval: UpdateCheckInterval,
    #[cfg_attr(feature = "export", ts(type = "number | null"))]
    pub last_update_check_time: Option<u64>,
    #[cfg_attr(feature = "export", ts(skip))]
    pub last_known_update_version: Option<String>,
    #[cfg_attr(feature = "export", ts(skip))]
    pub last_known_update_release_url: Option<String>,
    pub auto_update_enabled: bool,
    pub youtube_visibility: YoutubeVisibility,
    pub youtube_title_template: String,
    pub youtube_description_template: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            stop_replay_buffer_when_monitor_stopped: false,
            stop_replay_buffer_prompt_shown: false,
            monitor_design: DEFAULT_MONITOR_DESIGN,
            show_monitor_fps: false,
            show_developer_settings: false,
            show_source_previews: true,
            last_used_source_name: None,
            welcome_modal_shown: false,
            completed_output_path: String::new(),
            recent_run_limit: DEFAULT_RECENT_RUN_LIMIT,
            clip_filename_template: DEFAULT_CLIP_FILENAME_TEMPLATE.to_owned(),
            pre_run_padding_secs: DEFAULT_PRE_RUN_PADDING_SECS,
            post_run_padding_secs: DEFAULT_POST_RUN_PADDING_SECS,
            discord_notifications_enabled: true,
            discord_webhook_url: String::new(),
            streaming_started_message_template: DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE
                .to_owned(),
            streaming_stopped_message_template: DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE
                .to_owned(),
            update_check_interval: DEFAULT_UPDATE_CHECK_INTERVAL,
            last_update_check_time: None,
            last_known_update_version: None,
            last_known_update_release_url: None,
            auto_update_enabled: false,
            youtube_visibility: DEFAULT_YOUTUBE_VISIBILITY,
            youtube_title_template: DEFAULT_YOUTUBE_TITLE_TEMPLATE.to_owned(),
            youtube_description_template: DEFAULT_YOUTUBE_DESCRIPTION_TEMPLATE.to_owned(),
        }
    }
}

impl AppSettings {
    pub fn from_json_value(value: Value) -> serde_json::Result<Self> {
        serde_json::from_value(value).map(Self::normalized)
    }

    pub fn normalized(mut self) -> Self {
        let defaults = Self::default();
        self.last_used_source_name = self
            .last_used_source_name
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        self.recent_run_limit = self.recent_run_limit.clamp(1, MAX_RECENT_RUN_LIMIT);
        if self.clip_filename_template.is_empty() {
            self.clip_filename_template = defaults.clip_filename_template;
        }
        self.pre_run_padding_secs =
            non_negative_f64(self.pre_run_padding_secs, defaults.pre_run_padding_secs);
        self.post_run_padding_secs =
            non_negative_f64(self.post_run_padding_secs, defaults.post_run_padding_secs);
        self.streaming_started_message_template = non_empty_template(
            self.streaming_started_message_template,
            defaults.streaming_started_message_template,
        );
        self.streaming_stopped_message_template = non_empty_template(
            self.streaming_stopped_message_template,
            defaults.streaming_stopped_message_template,
        );
        self.youtube_title_template =
            non_empty_template(self.youtube_title_template, defaults.youtube_title_template);
        self.youtube_description_template = non_empty_template(
            self.youtube_description_template,
            defaults.youtube_description_template,
        );
        self
    }
}

fn non_negative_f64(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        fallback
    }
}

fn non_empty_template(value: String, fallback: String) -> String {
    if value.trim().is_empty() {
        fallback
    } else {
        value
    }
}
