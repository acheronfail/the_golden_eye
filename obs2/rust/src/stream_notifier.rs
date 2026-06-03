use anyhow::Context;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::http::{AppState, OAUTH_CALLBACK_PATH, SERVER_PORT};

const KEYRING_SERVICE: &str = "the-golden-eye";
const KEYRING_ENTRY: &str = "youtube-oauth-tokens";

fn redirect_uri() -> String {
    format!("http://localhost:{SERVER_PORT}{OAUTH_CALLBACK_PATH}")
}

/// Percent-encodes the redirect URI for embedding in an OAuth query parameter.
fn redirect_uri_encoded() -> String {
    redirect_uri().replace(':', "%3A").replace('/', "%2F")
}

// ── Serialisation types ───────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
struct OAuthTokens {
    access_token: String,
    refresh_token: Option<String>,
}

/// Subset of what the Google token endpoint returns.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct LiveBroadcastResponse {
    items: Option<Vec<LiveBroadcastItem>>,
}

#[derive(Deserialize)]
struct LiveBroadcastItem {
    id: String,
    snippet: Option<LiveBroadcastSnippet>,
}

#[derive(Deserialize)]
struct LiveBroadcastSnippet {
    #[serde(rename = "actualEndTime")]
    actual_end_time: Option<String>,
}

// ── Token persistence ─────────────────────────────────────────────────────────

fn load_tokens() -> Option<OAuthTokens> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ENTRY).ok()?;
    let json = entry.get_password().ok()?;
    serde_json::from_str(&json).ok()
}

fn save_tokens(tokens: &OAuthTokens) {
    let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ENTRY) else {
        tracing::error!("failed to create keyring entry");
        return;
    };
    match serde_json::to_string(tokens) {
        Ok(json) => {
            if let Err(e) = entry.set_password(&json) {
                tracing::error!("failed to save tokens to keyring: {e}");
            }
        }
        Err(e) => tracing::error!("failed to serialise tokens: {e}"),
    }
}

// ── OAuth flow ────────────────────────────────────────────────────────────────

async fn exchange_code(
    client: &reqwest::Client,
    code: &str,
    client_id: &str,
    client_secret: &str,
) -> anyhow::Result<OAuthTokens> {
    let ruri = redirect_uri();
    let resp: TokenResponse = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", ruri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(OAuthTokens {
        access_token: resp.access_token.context("no access_token in token exchange response")?,
        refresh_token: resp.refresh_token,
    })
}

async fn do_token_refresh(
    client: &reqwest::Client,
    refresh_token: &str,
    client_id: &str,
    client_secret: &str,
) -> anyhow::Result<String> {
    let resp: TokenResponse = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("refresh_token", refresh_token),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    resp.access_token.context("no access_token in refresh response")
}

/// Registers a one-shot receiver in the app state, opens the browser for the
/// Google OAuth consent screen, and waits for the axum `/oauth/callback` route
/// to send the authorisation code back through the channel.
async fn run_oauth_flow(
    client: &reqwest::Client,
    state: &AppState,
    client_id: &str,
    client_secret: &str,
) -> anyhow::Result<OAuthTokens> {
    let (tx, rx) = oneshot::channel::<String>();

    {
        let mut pending = state.oauth_pending.lock().await;
        *pending = Some(tx);
    }

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth\
         ?access_type=offline\
         &scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fyoutube.readonly\
         &include_granted_scopes=true\
         &response_type=code\
         &client_id={client_id}\
         &redirect_uri={}",
        redirect_uri_encoded()
    );

    tracing::info!("opening browser for YouTube OAuth");
    if let Err(e) = std::process::Command::new("open").arg(&auth_url).spawn() {
        tracing::warn!("failed to open browser automatically: {e}");
        tracing::info!("please open the following URL manually:\n{auth_url}");
    }

    let code = rx.await.context("OAuth channel closed before receiving code")?;
    tracing::info!("received OAuth authorisation code");

    let tokens = exchange_code(client, &code, client_id, client_secret).await?;
    save_tokens(&tokens);
    Ok(tokens)
}

// ── YouTube API ───────────────────────────────────────────────────────────────

/// Returns `Some(broadcast_id)` for an active (not-yet-ended) live broadcast,
/// or `None` if there is nothing live right now.
async fn fetch_live_broadcast(
    client: &reqwest::Client,
    access_token: &str,
) -> anyhow::Result<Option<String>> {
    let resp: LiveBroadcastResponse = client
        .get("https://www.googleapis.com/youtube/v3/liveBroadcasts?part=snippet&mine=true&maxResults=1")
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let Some(items) = resp.items else {
        return Ok(None);
    };
    let Some(item) = items.into_iter().next() else {
        return Ok(None);
    };

    // Skip broadcasts that have already ended.
    if item.snippet.as_ref().and_then(|s| s.actual_end_time.as_ref()).is_some() {
        return Ok(None);
    }

    Ok(Some(item.id))
}

/// Attempts to fetch the live broadcast, refreshing or re-authing as needed.
async fn fetch_live_broadcast_with_retry(
    client: &reqwest::Client,
    tokens: &mut OAuthTokens,
    client_id: &str,
    client_secret: &str,
    state: &AppState,
) -> anyhow::Result<Option<String>> {
    // Happy path.
    if let Ok(result) = fetch_live_broadcast(client, &tokens.access_token).await {
        return Ok(result);
    }

    // Try refreshing the access token first.
    if let Some(rt) = tokens.refresh_token.clone() {
        match do_token_refresh(client, &rt, client_id, client_secret).await {
            Ok(new_access) => {
                tokens.access_token = new_access;
                save_tokens(tokens);
                if let Ok(result) = fetch_live_broadcast(client, &tokens.access_token).await {
                    return Ok(result);
                }
                tracing::warn!("broadcast fetch still failed after token refresh, re-running OAuth flow");
            }
            Err(e) => tracing::warn!("token refresh failed: {e}, re-running OAuth flow"),
        }
    }

    // Full re-auth as a last resort.
    *tokens = run_oauth_flow(client, state, client_id, client_secret).await?;
    fetch_live_broadcast(client, &tokens.access_token).await
}

// ── Public entry point ────────────────────────────────────────────────────────

pub async fn run(state: AppState) {
    if let Err(e) = run_inner(state).await {
        tracing::error!("stream notifier error: {e:#}");
    }
}

async fn run_inner(state: AppState) -> anyhow::Result<()> {
    let client_id = std::env::var("GOOGLE_CLIENT_ID").context("GOOGLE_CLIENT_ID not set")?;
    let client_secret =
        std::env::var("GOOGLE_CLIENT_SECRET").context("GOOGLE_CLIENT_SECRET not set")?;
    let discord_webhook_url =
        std::env::var("DISCORD_WEBHOOK_URL").context("DISCORD_WEBHOOK_URL not set")?;

    let client = reqwest::Client::new();

    // Load cached tokens, or run the OAuth flow if they are absent/incomplete.
    let mut tokens = match load_tokens() {
        Some(t) if t.refresh_token.is_some() => {
            tracing::info!("loaded cached OAuth tokens");
            t
        }
        _ => {
            tracing::info!("no valid cached tokens found, starting OAuth flow");
            run_oauth_flow(&client, &state, &client_id, &client_secret).await?
        }
    };

    let Some(broadcast_id) = fetch_live_broadcast_with_retry(
        &client,
        &mut tokens,
        &client_id,
        &client_secret,
        &state,
    )
    .await?
    else {
        tracing::info!("no active YouTube live broadcast found, skipping Discord notification");
        return Ok(());
    };

    let broadcast_url = format!("https://youtu.be/{broadcast_id}");
    tracing::info!("posting Discord notification for {broadcast_url}");

    client
        .post(&discord_webhook_url)
        .json(&serde_json::json!({ "content": format!("Now streaming: {broadcast_url}") }))
        .send()
        .await
        .context("failed to send Discord webhook request")?
        .error_for_status()
        .context("Discord webhook returned an error")?;

    Ok(())
}
