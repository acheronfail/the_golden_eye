use std::time::SystemTime;

use anyhow::Context;
use serde::Deserialize;

use crate::http::{AppState, StreamMessage};
use crate::template_tokens::{format_iso_local, format_iso_utc, system_time_unix_seconds};

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

#[cfg(test)]
#[path = "stream_notifier_test.rs"]
mod stream_notifier_test;
