use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use serde::Deserialize;

use crate::http::{AppState, StreamMessage};

pub const DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE: &str = "🟢 Bond is now streaming at: {broadcast_url}";
pub const DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE: &str =
    "🔴 Bond stopped streaming at <t:{unix_seconds}:F>: {broadcast_url}";

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct ServiceSettings {
    service: Option<String>,
    server: Option<String>,
    protocol: Option<String>,
    stream_key_link: Option<String>,
    multitrack_video_name: Option<String>,
    multitrack_video_disclaimer: Option<String>,
    key: Option<String>,
    broadcast_id: Option<String>,
}

/// The subset of a Discord message object we care about (returned when posting
/// a webhook with `wait=true`).
#[derive(Debug, Deserialize)]
pub struct DiscordMessage {
    pub id: String,
}

// ── Public entry point ────────────────────────────────────────────────────────

pub async fn run(state: AppState, service_settings_json: String) {
    if let Err(e) = run_inner(state, service_settings_json).await {
        tracing::error!("stream notifier error: {e:#}");
    }
}

pub async fn stop(state: AppState) {
    if let Err(e) = stop_inner(state).await {
        tracing::error!("stream notifier stop error: {e:#}");
    }
}

async fn stop_inner(state: AppState) -> anyhow::Result<()> {
    // The "now streaming" message we posted on start, if any. Take it so a
    // subsequent stop without an intervening start doesn't re-edit it.
    let Some(message) = state.stream_message.lock().await.take() else {
        tracing::info!("no recorded stream message to update, skipping Discord edit");
        return Ok(());
    };

    let notification_options = state.settings.get_notification_options();
    if !notification_options.enabled {
        tracing::info!("stream notifier is disabled, skipping stop");
        return Ok(());
    }

    let client = reqwest::Client::new();
    let edit_url = format!("{}/messages/{}", message.webhook_url.trim_end_matches('/'), message.id);
    let content = render_notification_template(
        &notification_options.streaming_stopped_message_template,
        &message.broadcast_url,
        SystemTime::now(),
    );

    tracing::info!("editing Discord message {} to mark the stream as ended", message.id);

    client
        .patch(&edit_url)
        .json(&serde_json::json!({
            "content": content,
            // SUPPRESS_EMBEDS (1 << 2): hide the auto-generated YouTube link preview.
            "flags": 4
        }))
        .send()
        .await
        .context("failed to send Discord webhook edit request")?
        .error_for_status()
        .context("Discord webhook edit returned an error")?;

    Ok(())
}

async fn run_inner(state: AppState, service_settings_json: String) -> anyhow::Result<()> {
    let notification_options = state.settings.get_notification_options();
    if !notification_options.enabled {
        tracing::info!("stream notifier is disabled, skipping start");
        return Ok(());
    }

    if notification_options.discord_webhook_url.is_empty() {
        tracing::info!("stream notifier is disabled (missing configuration), skipping start");
        return Ok(());
    }
    let discord_webhook_url = notification_options.discord_webhook_url;

    let settings: ServiceSettings = serde_json::from_str(&service_settings_json)
        .with_context(|| format!("failed to parse service settings JSON: {service_settings_json}"))?;

    let Some(broadcast_id) = settings.broadcast_id.filter(|id| !id.is_empty()) else {
        tracing::info!("service settings did not include broadcast_id, skipping Discord notification");
        return Ok(());
    };

    let client = reqwest::Client::new();

    let broadcast_url = format!("https://youtu.be/{broadcast_id}");
    let content = render_notification_template(
        &notification_options.streaming_started_message_template,
        &broadcast_url,
        SystemTime::now(),
    );
    tracing::info!(
        %broadcast_url,
        content = content,
        "posting Discord notification"
    );

    // `wait=true` makes Discord return the created message so we can grab its id
    // and edit it in place when the stream stops.
    let post_url = format!("{}?wait=true", discord_webhook_url.trim_end_matches('/'));
    let message: DiscordMessage = client
        .post(&post_url)
        .json(&serde_json::json!({ "content": content }))
        .send()
        .await
        .context("failed to send Discord webhook request")?
        .error_for_status()
        .context("Discord webhook returned an error")?
        .json()
        .await
        .context("failed to parse Discord webhook response")?;

    *state.stream_message.lock().await =
        Some(StreamMessage { id: message.id, broadcast_url, webhook_url: discord_webhook_url });

    Ok(())
}

fn render_notification_template(template: &str, broadcast_url: &str, time: SystemTime) -> String {
    let unix_seconds = system_time_unix_seconds(time);
    let timestamp = format_iso_utc(time);
    let timestamp_local = format_iso_local(time);

    template
        .replace("{broadcast_url}", broadcast_url)
        .replace("{timestamp}", &timestamp)
        .replace("{timestamp_local}", &timestamp_local)
        .replace("{unix_seconds}", &unix_seconds.to_string())
}

fn system_time_unix_seconds(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
        Err(err) => {
            let duration = err.duration();
            let seconds = i64::try_from(duration.as_secs()).unwrap_or(i64::MAX);
            if duration.subsec_nanos() == 0 { -seconds } else { -seconds - 1 }
        }
    }
}

fn div_floor(a: i64, b: i64) -> i64 {
    let quotient = a / b;
    let remainder = a % b;
    if remainder != 0 && ((remainder > 0) != (b > 0)) { quotient - 1 } else { quotient }
}

fn utc_from_unix_seconds(seconds: i64) -> (i64, i64, i64, i64, i64, i64) {
    let days = div_floor(seconds, 86_400);
    let seconds_of_day = seconds - days * 86_400;
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    // Howard Hinnant's civil-from-days conversion, using Unix day zero.
    let z = days + 719_468;
    let era = div_floor(z, 146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };

    (year, month, day, hour, minute, second)
}

fn format_iso_utc(time: SystemTime) -> String {
    let (year, month, day, hour, minute, second) = utc_from_unix_seconds(system_time_unix_seconds(time));
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(unix)]
fn format_iso_local(time: SystemTime) -> String {
    let seconds = system_time_unix_seconds(time);
    let time_t = seconds as libc::time_t;
    let mut local_tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    let local_tm = unsafe {
        if libc::localtime_r(&time_t, local_tm.as_mut_ptr()).is_null() {
            return format_iso_utc(time);
        }
        local_tm.assume_init()
    };
    let offset = local_tm.tm_gmtoff;
    let sign = if offset < 0 { '-' } else { '+' };
    let offset = offset.abs();
    let offset_hour = offset / 3_600;
    let offset_minute = (offset % 3_600) / 60;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{sign}{offset_hour:02}:{offset_minute:02}",
        local_tm.tm_year + 1900,
        local_tm.tm_mon + 1,
        local_tm.tm_mday,
        local_tm.tm_hour,
        local_tm.tm_min,
        local_tm.tm_sec,
    )
}

#[cfg(not(unix))]
fn format_iso_local(time: SystemTime) -> String {
    format_iso_utc(time)
}

#[cfg(test)]
#[path = "stream_notifier_test.rs"]
mod stream_notifier_test;
