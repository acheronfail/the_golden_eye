use std::time::SystemTime;

use anyhow::Context;
use serde::Deserialize;

use crate::http::AppState;
use crate::http::StreamMessage;

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
    let Some(discord_webhook_url) = state.config.discord_webhook_url.as_ref() else {
        tracing::info!("stream notifier is disabled (missing configuration), skipping stop");
        return Ok(());
    };

    // The "now streaming" message we posted on start, if any. Take it so a
    // subsequent stop without an intervening start doesn't re-edit it.
    let Some(message) = state.stream_message.lock().await.take() else {
        tracing::info!("no recorded stream message to update, skipping Discord edit");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let edit_url = format!("{}/messages/{}", discord_webhook_url.trim_end_matches('/'), message.id);
    let unix_seconds = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);

    tracing::info!("editing Discord message {} to mark the stream as ended", message.id);

    client
        .patch(&edit_url)
        .json(&serde_json::json!({
            "content": match state.config.discord_message_name.as_ref() {
                Some(name) => format!("🔴 {name} has stopped streaming at <t:{unix_seconds}:F>: {}", message.broadcast_url),
                None => format!("🔴 Stream has ended at <t:{unix_seconds}:F>: {}", message.broadcast_url),
            },
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
    let Some(discord_webhook_url) = state.config.discord_webhook_url.as_ref() else {
        tracing::info!("stream notifier is disabled (missing configuration), skipping start");
        return Ok(());
    };

    let settings: ServiceSettings = serde_json::from_str(&service_settings_json)
        .with_context(|| format!("failed to parse service settings JSON: {service_settings_json}"))?;

    let Some(broadcast_id) = settings.broadcast_id.filter(|id| !id.is_empty()) else {
        tracing::info!("service settings did not include broadcast_id, skipping Discord notification");
        return Ok(());
    };

    let client = reqwest::Client::new();

    let broadcast_url = format!("https://youtu.be/{broadcast_id}");
    let content = match state.config.discord_message_name.as_ref() {
        Some(name) => format!("🟢 {name} is now streaming at: {broadcast_url}"),
        None => format!("🟢 Now streaming: {broadcast_url}"),
    };
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

    *state.stream_message.lock().await = Some(StreamMessage { id: message.id, broadcast_url });

    Ok(())
}
