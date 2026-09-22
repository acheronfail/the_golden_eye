//! The consent flow owns its pending callback, cancellation, and credential exchange.
use std::time::SystemTime;

use anyhow::{Context, anyhow};
use base64::Engine;
use reqwest::StatusCode;
use reqwest::header::AUTHORIZATION;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::{broadcast, oneshot};

use super::credentials::YoutubeTokens;
use super::{USER_AGENT, YoutubeAccount, YoutubeStatus, YoutubeUploadStore, unix_secs};
use crate::app::AppEvent;

pub(crate) const OAUTH_CALLBACK_PATH: &str = "/oauth/callback";

pub(super) struct PendingOAuth {
    state: String,
    tx: oneshot::Sender<String>,
}

#[derive(Debug)]
pub(crate) enum ConnectError {
    Disabled,
    Unconfigured,
    Cancelled,
    ExchangeFailed,
    #[cfg_attr(feature = "test-hooks", allow(dead_code))]
    BrowserUnavailable,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CallbackError {
    MissingCode,
    NoPendingFlow,
    StateMismatch,
}

#[derive(Debug)]
pub(crate) enum DisconnectError {
    Disabled,
    DeleteFailed,
}

impl YoutubeUploadStore {
    pub(crate) async fn connect(&self, events: &broadcast::Sender<AppEvent>) -> Result<YoutubeStatus, ConnectError> {
        if !self.enabled() {
            return Err(ConnectError::Disabled);
        }
        if !self.oauth_configured() {
            return Err(ConnectError::Unconfigured);
        }
        let state = new_oauth_state();
        let auth_url = self.config().authorization_url(&state);
        let (tx, rx) = oneshot::channel();
        *self.pending_oauth.lock().await = Some(PendingOAuth { state, tx });
        open_consent_page(&auth_url).await?;
        let code = rx.await.map_err(|_| ConnectError::Cancelled)?;
        self.exchange_code(&code).await.map_err(|err| {
            tracing::error!("YouTube OAuth exchange failed: {err:#}");
            ConnectError::ExchangeFailed
        })?;
        let status = self.status();
        let _ = events.send(AppEvent::YoutubeStatusChanged { status: status.clone() });
        Ok(status)
    }

    pub(crate) fn disconnect(&self, events: &broadcast::Sender<AppEvent>) -> Result<YoutubeStatus, DisconnectError> {
        if !self.enabled() {
            return Err(DisconnectError::Disabled);
        }
        self.credential_store.delete().map_err(|err| {
            tracing::error!("failed to disconnect YouTube: {err:#}");
            DisconnectError::DeleteFailed
        })?;
        let status = self.status();
        let _ = events.send(AppEvent::YoutubeStatusChanged { status: status.clone() });
        Ok(status)
    }

    pub(crate) async fn cancel_connect(&self) {
        self.pending_oauth.lock().await.take();
    }

    pub(crate) async fn accept_oauth_callback(
        &self,
        code: Option<String>,
        state: Option<String>,
    ) -> Result<(), CallbackError> {
        let code = code.ok_or(CallbackError::MissingCode)?;
        let mut pending = self.pending_oauth.lock().await;
        let flow = pending.take().ok_or(CallbackError::NoPendingFlow)?;
        if state.as_deref() != Some(flow.state.as_str()) {
            return Err(CallbackError::StateMismatch);
        }
        let _ = flow.tx.send(code);
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
    token_type: Option<String>,
}

impl YoutubeUploadStore {
    pub async fn exchange_code(&self, code: &str) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let mut form = vec![
            ("client_id", self.config.client_id.as_str()),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", self.config.redirect_uri.as_str()),
        ];
        if !self.config.client_secret.is_empty() {
            form.push(("client_secret", self.config.client_secret.as_str()));
        }
        let response = client
            .post(&self.config.token_url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .form(&form)
            .send()
            .await
            .context("exchanging YouTube OAuth code")?;
        let status = response.status();
        let body = response.text().await.context("reading YouTube token response")?;
        if !status.is_success() {
            anyhow::bail!("YouTube token exchange failed with {status}: {body}");
        }
        let token: TokenResponse = serde_json::from_str(&body).context("parsing YouTube token response")?;
        let refresh_token = token
            .refresh_token
            .or_else(|| self.credential_store.load().ok().flatten().map(|t| t.refresh_token))
            .ok_or_else(|| anyhow!("YouTube did not return a refresh token"))?;
        let access_token = token.access_token;
        let account = match access_token.as_deref() {
            Some(token) => self.fetch_userinfo(token).await.ok(),
            None => None,
        };
        let tokens = YoutubeTokens {
            refresh_token,
            access_token,
            expires_at_unix_secs: token.expires_in.map(|seconds| unix_secs(SystemTime::now()) + seconds),
            scope: token.scope,
            token_type: token.token_type,
            account,
        };
        self.credential_store.save(&tokens)
    }

    pub async fn access_token(&self) -> anyhow::Result<String> {
        let mut tokens = self.credential_store.load()?.ok_or_else(|| anyhow!("YouTube is not connected"))?;
        if let Some(token) = tokens.access_token_valid() {
            return Ok(token.to_owned());
        }
        let refreshed = self.refresh_token(&tokens.refresh_token).await?;
        tokens.access_token = refreshed.access_token;
        tokens.expires_at_unix_secs = refreshed.expires_in.map(|seconds| unix_secs(SystemTime::now()) + seconds);
        tokens.scope = refreshed.scope.or(tokens.scope);
        tokens.token_type = refreshed.token_type.or(tokens.token_type);
        if let Some(access_token) = tokens.access_token.as_deref()
            && tokens.account.is_none()
        {
            tokens.account = self.fetch_userinfo(access_token).await.ok();
        }
        self.credential_store.save(&tokens)?;
        tokens.access_token.ok_or_else(|| anyhow!("YouTube refresh response did not include an access token"))
    }

    async fn fetch_userinfo(&self, access_token: &str) -> anyhow::Result<YoutubeAccount> {
        let response = reqwest::Client::new()
            .get(&self.config.userinfo_url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(AUTHORIZATION, format!("Bearer {access_token}"))
            .send()
            .await
            .context("fetching Google account info")?;
        let status = response.status();
        let body = response.text().await.context("reading Google account info response")?;
        if !status.is_success() {
            anyhow::bail!("Google account info request failed with {status}: {body}");
        }
        serde_json::from_str(&body).context("parsing Google account info")
    }

    async fn refresh_token(&self, refresh_token: &str) -> anyhow::Result<TokenResponse> {
        let client = reqwest::Client::new();
        let mut form = vec![
            ("client_id", self.config.client_id.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ];
        if !self.config.client_secret.is_empty() {
            form.push(("client_secret", self.config.client_secret.as_str()));
        }
        let response = client
            .post(&self.config.token_url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .form(&form)
            .send()
            .await
            .context("refreshing YouTube access token")?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::BAD_REQUEST {
            let _ = self.credential_store.delete();
        }
        let body = response.text().await.context("reading YouTube refresh response")?;
        if !status.is_success() {
            anyhow::bail!("YouTube token refresh failed with {status}: {body}");
        }
        serde_json::from_str(&body).context("parsing YouTube refresh response")
    }
}

#[cfg(not(feature = "test-hooks"))]
async fn open_consent_page(auth_url: &str) -> Result<(), ConnectError> {
    let auth_url = auth_url.to_owned();
    tokio::task::spawn_blocking(move || crate::desktop::browser::open_url(&auth_url))
        .await
        .map_err(|err| {
            tracing::error!("failed to join browser opener task: {err:#}");
            ConnectError::BrowserUnavailable
        })?
        .map_err(|err| {
            tracing::error!("failed to open YouTube OAuth URL: {err:#}");
            ConnectError::BrowserUnavailable
        })?;
    Ok(())
}

#[cfg(feature = "test-hooks")]
async fn open_consent_page(_auth_url: &str) -> Result<(), ConnectError> {
    Ok(())
}

fn new_oauth_state() -> String {
    #[cfg(feature = "test-hooks")]
    if let Some(state) = crate::config::test_oauth_state() {
        return state;
    }
    let mut hasher = Sha256::new();
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos().to_le_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
}
