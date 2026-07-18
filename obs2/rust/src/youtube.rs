use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, anyhow};
use axum::http::StatusCode;
use base64::Engine;
use keyring::Entry;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, HeaderMap, HeaderValue, LOCATION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::config;
use crate::ffmpeg::ClipMetadata;
use crate::settings::{AppSettings, YoutubeVisibility};
use crate::template_tokens::RunTemplateTokens;

const YOUTUBE_UPLOAD_SCOPE: &str = "openid email profile https://www.googleapis.com/auth/youtube.upload";
const KEYRING_SERVICE: &str = "the-golden-eye.youtube";
const KEYRING_ACCOUNT: &str = "oauth-tokens";
const UPLOAD_CONCURRENCY: usize = 2;
// Keep chunks small enough that typical run clips emit visible progress events.
const CHUNK_SIZE: u64 = 1024 * 1024;
const USER_AGENT: &str = "the-golden-eye-obs-plugin";
pub(crate) const HISTORY_FILE_NAME: &str = "youtube_uploads.json";
const TOKEN_FILE_NAME: &str = "youtube_tokens.json";

#[derive(Debug, Clone)]
pub struct YoutubeConfig {
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub upload_url: String,
    pub userinfo_url: String,
    pub redirect_uri: String,
    pub scope: String,
    pub enabled: bool,
}

impl YoutubeConfig {
    pub fn from_env() -> Self {
        let endpoints = config::YoutubeEndpoints::resolve();
        let client_secret = youtube_client_secret();
        let enabled = config::youtube_enabled(&endpoints, &client_secret);
        Self {
            client_id: endpoints.client_id,
            client_secret,
            auth_url: endpoints.auth_url,
            token_url: endpoints.token_url,
            upload_url: endpoints.upload_url,
            userinfo_url: endpoints.userinfo_url,
            redirect_uri: endpoints.redirect_uri,
            scope: YOUTUBE_UPLOAD_SCOPE.to_owned(),
            enabled,
        }
    }

    pub fn configured(&self) -> bool {
        !self.client_id.trim().is_empty() && !self.client_id.starts_with("TODO_CONFIGURE_")
    }

    pub fn authorization_url(&self, state: &str) -> String {
        let mut url = reqwest::Url::parse(&self.auth_url).expect("valid YouTube OAuth URL");
        url.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", &self.scope)
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent")
            .append_pair("state", state);
        url.into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YoutubeTokens {
    pub refresh_token: String,
    pub access_token: Option<String>,
    pub expires_at_unix_secs: Option<u64>,
    pub scope: Option<String>,
    pub token_type: Option<String>,
    pub account: Option<YoutubeAccount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YoutubeAccount {
    pub email: Option<String>,
    pub name: Option<String>,
    pub picture: Option<String>,
}

impl YoutubeTokens {
    fn access_token_valid(&self) -> Option<&str> {
        let token = self.access_token.as_deref()?;
        let expires_at = self.expires_at_unix_secs?;
        let now = unix_secs(SystemTime::now());
        (expires_at > now + 60).then_some(token)
    }
}

pub trait YoutubeCredentialStore: Send + Sync {
    fn load(&self) -> anyhow::Result<Option<YoutubeTokens>>;
    fn save(&self, tokens: &YoutubeTokens) -> anyhow::Result<()>;
    fn delete(&self) -> anyhow::Result<()>;
}

#[derive(Debug, Clone)]
pub struct KeyringYoutubeCredentialStore {
    service: String,
    account: String,
}

impl Default for KeyringYoutubeCredentialStore {
    fn default() -> Self {
        Self { service: KEYRING_SERVICE.to_owned(), account: KEYRING_ACCOUNT.to_owned() }
    }
}

impl KeyringYoutubeCredentialStore {
    #[cfg(test)]
    pub fn test_account(suffix: &str) -> Self {
        Self { service: KEYRING_SERVICE.to_owned(), account: format!("oauth-tokens-test-{suffix}") }
    }

    fn entry(&self) -> anyhow::Result<Entry> {
        Entry::new(&self.service, &self.account).context("opening YouTube keyring entry")
    }
}

impl YoutubeCredentialStore for KeyringYoutubeCredentialStore {
    fn load(&self) -> anyhow::Result<Option<YoutubeTokens>> {
        let entry = self.entry()?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(serde_json::from_str(&secret).context("parsing YouTube keyring tokens")?)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(err).context("reading YouTube keyring tokens"),
        }
    }

    fn save(&self, tokens: &YoutubeTokens) -> anyhow::Result<()> {
        let secret = serde_json::to_string(tokens).context("serializing YouTube tokens")?;
        self.entry()?.set_password(&secret).context("saving YouTube keyring tokens")
    }

    fn delete(&self) -> anyhow::Result<()> {
        let entry = self.entry()?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(err).context("deleting YouTube keyring tokens"),
        }
    }
}

#[cfg(test)]
#[allow(dead_code)]
#[derive(Default)]
pub struct MemoryYoutubeCredentialStore {
    tokens: Mutex<Option<YoutubeTokens>>,
}

#[cfg(test)]
impl YoutubeCredentialStore for MemoryYoutubeCredentialStore {
    fn load(&self) -> anyhow::Result<Option<YoutubeTokens>> {
        Ok(self.tokens.lock().unwrap().clone())
    }

    fn save(&self, tokens: &YoutubeTokens) -> anyhow::Result<()> {
        *self.tokens.lock().unwrap() = Some(tokens.clone());
        Ok(())
    }

    fn delete(&self) -> anyhow::Result<()> {
        *self.tokens.lock().unwrap() = None;
        Ok(())
    }
}

#[cfg(any(test, feature = "test-hooks"))]
#[derive(Default)]
struct FailingYoutubeCredentialStore;

#[cfg(any(test, feature = "test-hooks"))]
impl YoutubeCredentialStore for FailingYoutubeCredentialStore {
    fn load(&self) -> anyhow::Result<Option<YoutubeTokens>> {
        Err(anyhow!("keyring unavailable"))
    }

    fn save(&self, _tokens: &YoutubeTokens) -> anyhow::Result<()> {
        Err(anyhow!("keyring unavailable"))
    }

    fn delete(&self) -> anyhow::Result<()> {
        Err(anyhow!("keyring unavailable"))
    }
}

#[derive(Debug, Clone)]
struct FileYoutubeCredentialStore {
    path: PathBuf,
}

impl YoutubeCredentialStore for FileYoutubeCredentialStore {
    fn load(&self) -> anyhow::Result<Option<YoutubeTokens>> {
        match fs::read(&self.path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes).context("parsing YouTube token file")?)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err).with_context(|| format!("reading {}", self.path.display())),
        }
    }

    fn save(&self, tokens: &YoutubeTokens) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::write(&self.path, serde_json::to_vec_pretty(tokens)?)
            .with_context(|| format!("writing {}", self.path.display()))
    }

    fn delete(&self) -> anyhow::Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err).with_context(|| format!("deleting {}", self.path.display())),
        }
    }
}

#[derive(Clone)]
struct FallbackYoutubeCredentialStore {
    primary: Arc<dyn YoutubeCredentialStore>,
    file: FileYoutubeCredentialStore,
}

impl FallbackYoutubeCredentialStore {
    fn warn_fallback(&self, action: &str, err: &anyhow::Error) {
        tracing::warn!(
            action,
            fallback_path = %self.file.path.display(),
            "YouTube keyring unavailable; using file token store: {err:#}"
        );
    }
}

impl YoutubeCredentialStore for FallbackYoutubeCredentialStore {
    fn load(&self) -> anyhow::Result<Option<YoutubeTokens>> {
        match self.primary.load() {
            Ok(Some(tokens)) => Ok(Some(tokens)),
            Ok(None) => self.file.load(),
            Err(err) => {
                self.warn_fallback("load", &err);
                self.file.load()
            }
        }
    }

    fn save(&self, tokens: &YoutubeTokens) -> anyhow::Result<()> {
        match self.primary.save(tokens) {
            Ok(()) => {
                let _ = self.file.delete();
                Ok(())
            }
            Err(err) => {
                self.warn_fallback("save", &err);
                self.file.save(tokens)
            }
        }
    }

    fn delete(&self) -> anyhow::Result<()> {
        let keyring_result = self.primary.delete();
        let file_result = self.file.delete();
        match (keyring_result, file_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(err), Ok(())) => {
                self.warn_fallback("delete", &err);
                Ok(())
            }
            (Ok(()), Err(err)) | (Err(_), Err(err)) => Err(err),
        }
    }
}

fn youtube_credential_store(settings_path: &Path) -> Arc<dyn YoutubeCredentialStore> {
    #[cfg(feature = "test-hooks")]
    if let Some(path) = config::token_file_override() {
        return Arc::new(FileYoutubeCredentialStore { path });
    }
    #[cfg(feature = "test-hooks")]
    let primary: Arc<dyn YoutubeCredentialStore> = if config::force_keyring_failure() {
        Arc::new(FailingYoutubeCredentialStore)
    } else {
        Arc::new(KeyringYoutubeCredentialStore::default())
    };
    #[cfg(not(feature = "test-hooks"))]
    let primary: Arc<dyn YoutubeCredentialStore> = Arc::new(KeyringYoutubeCredentialStore::default());

    Arc::new(FallbackYoutubeCredentialStore {
        primary,
        file: FileYoutubeCredentialStore { path: settings_path.with_file_name(TOKEN_FILE_NAME) },
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadHistoryEntry {
    pub identity: ClipIdentity,
    pub video_id: String,
    pub video_url: String,
    pub uploaded_at: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipIdentity {
    pub path: String,
    pub size_bytes: u64,
    pub modified_unix_secs: Option<u64>,
    pub metadata_sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadHistory {
    pub entries: Vec<UploadHistoryEntry>,
}

#[derive(Clone)]
pub struct YoutubeUploadStore {
    inner: Arc<Mutex<YoutubeUploadInner>>,
    semaphore: Arc<Semaphore>,
    history_path: PathBuf,
    credential_store: Arc<dyn YoutubeCredentialStore>,
    config: YoutubeConfig,
}

struct YoutubeUploadInner {
    uploads: HashMap<String, YoutubeUploadStatus>,
    path_to_active_id: HashMap<String, String>,
}

impl YoutubeUploadStore {
    pub fn new(settings_path: &Path) -> Self {
        Self::with_parts(settings_path, youtube_credential_store(settings_path), YoutubeConfig::from_env())
    }

    pub fn with_parts(
        settings_path: &Path,
        credential_store: Arc<dyn YoutubeCredentialStore>,
        config: YoutubeConfig,
    ) -> Self {
        let history_path = settings_path.with_file_name(HISTORY_FILE_NAME);
        Self {
            inner: Arc::new(Mutex::new(YoutubeUploadInner {
                uploads: HashMap::new(),
                path_to_active_id: HashMap::new(),
            })),
            semaphore: Arc::new(Semaphore::new(UPLOAD_CONCURRENCY)),
            history_path,
            credential_store,
            config,
        }
    }

    pub fn oauth_configured(&self) -> bool {
        self.config.configured()
    }

    pub fn enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn connected(&self) -> bool {
        self.credential_store.load().ok().flatten().is_some()
    }

    pub fn account(&self) -> Option<YoutubeAccount> {
        self.credential_store.load().ok().flatten().and_then(|tokens| tokens.account)
    }

    pub fn status(&self) -> YoutubeStatus {
        YoutubeStatus {
            enabled: self.enabled(),
            oauth_configured: self.oauth_configured(),
            connected: self.connected(),
            account: self.account(),
            uploads: self.uploads(),
            history: self.read_history().entries,
        }
    }

    pub fn uploads(&self) -> Vec<YoutubeUploadStatus> {
        let mut uploads = self.inner.lock().unwrap().uploads.values().cloned().collect::<Vec<_>>();
        uploads.sort_by(|a, b| a.started_at.cmp(&b.started_at));
        uploads
    }

    pub fn disconnect(&self) -> anyhow::Result<()> {
        self.credential_store.delete()
    }

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

    pub fn active_upload_for_display_path(&self, display_path: &str) -> Option<YoutubeUploadStatus> {
        let inner = self.inner.lock().unwrap();
        let id = inner.path_to_active_id.get(display_path)?;
        inner.uploads.get(id).cloned()
    }

    pub fn insert_queued_upload(
        &self,
        upload_path: &Path,
        display_path: String,
        title: String,
        description: String,
        total_bytes: u64,
    ) -> YoutubeUploadStatus {
        let path_string = display_path;
        let id = upload_id(upload_path, SystemTime::now());
        let file_name = upload_path.file_name().and_then(|s| s.to_str()).unwrap_or("clip").to_owned();
        let status = YoutubeUploadStatus {
            id: id.clone(),
            path: path_string.clone(),
            file_name,
            state: YoutubeUploadState::Queued,
            progress_bytes: 0,
            total_bytes: Some(total_bytes),
            progress_ratio: Some(0.0),
            video_id: None,
            video_url: None,
            error: None,
            title,
            description,
            started_at: now_iso(),
            finished_at: None,
        };
        let mut inner = self.inner.lock().unwrap();
        inner.path_to_active_id.insert(path_string, id.clone());
        inner.uploads.insert(id, status.clone());
        status
    }

    pub fn update_upload(
        &self,
        id: &str,
        update: impl FnOnce(&mut YoutubeUploadStatus),
    ) -> Option<YoutubeUploadStatus> {
        let mut inner = self.inner.lock().unwrap();
        let status = inner.uploads.get_mut(id)?;
        update(status);
        let finished = matches!(status.state, YoutubeUploadState::Uploaded | YoutubeUploadState::Failed);
        if finished {
            status.finished_at.get_or_insert_with(now_iso);
        }
        let cloned = status.clone();
        if finished {
            inner.path_to_active_id.retain(|_, active_id| active_id != id);
        }
        Some(cloned)
    }

    pub fn read_history(&self) -> UploadHistory {
        fs::read(&self.history_path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    pub fn append_history(&self, entry: UploadHistoryEntry) -> anyhow::Result<()> {
        let mut history = self.read_history();
        history.entries.retain(|item| item.identity != entry.identity);
        history.entries.push(entry);
        self.write_history(&history)
    }

    pub fn forget_for_display_path(&self, display_path: &str) -> anyhow::Result<usize> {
        let removed_history = self.forget_history_for_display_path(display_path)?;
        let removed_uploads = self.forget_retained_uploads_for_display_path(display_path);
        Ok(removed_history + removed_uploads)
    }

    fn forget_history_for_display_path(&self, display_path: &str) -> anyhow::Result<usize> {
        let mut history = self.read_history();
        let before = history.entries.len();
        history.entries.retain(|entry| !paths_match_for_current_platform(&entry.identity.path, display_path));
        let removed = before.saturating_sub(history.entries.len());
        if removed > 0 {
            self.write_history(&history)?;
        }
        Ok(removed)
    }

    fn forget_retained_uploads_for_display_path(&self, display_path: &str) -> usize {
        let mut inner = self.inner.lock().unwrap();
        let ids = inner
            .uploads
            .iter()
            .filter_map(|(id, upload)| {
                paths_match_for_current_platform(&upload.path, display_path).then_some(id.clone())
            })
            .collect::<Vec<_>>();
        for id in &ids {
            inner.uploads.remove(id);
        }
        inner
            .path_to_active_id
            .retain(|path, id| !paths_match_for_current_platform(path, display_path) && !ids.contains(id));
        ids.len()
    }

    fn write_history(&self, history: &UploadHistory) -> anyhow::Result<()> {
        if let Some(parent) = self.history_path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let bytes = serde_json::to_vec_pretty(history).context("serializing YouTube upload history")?;
        fs::write(&self.history_path, bytes).with_context(|| format!("writing {}", self.history_path.display()))
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

    pub fn semaphore(&self) -> Arc<Semaphore> {
        self.semaphore.clone()
    }

    pub fn config(&self) -> YoutubeConfig {
        self.config.clone()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YoutubeStatus {
    pub enabled: bool,
    pub oauth_configured: bool,
    pub connected: bool,
    pub account: Option<YoutubeAccount>,
    pub uploads: Vec<YoutubeUploadStatus>,
    pub history: Vec<UploadHistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YoutubeUploadStatus {
    pub id: String,
    pub path: String,
    pub file_name: String,
    pub state: YoutubeUploadState,
    pub progress_bytes: u64,
    pub total_bytes: Option<u64>,
    pub progress_ratio: Option<f64>,
    pub video_id: Option<String>,
    pub video_url: Option<String>,
    pub error: Option<String>,
    pub title: String,
    #[serde(skip_serializing)]
    pub description: String,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum YoutubeUploadState {
    Queued,
    Uploading,
    Processing,
    Uploaded,
    Failed,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
    token_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VideoInsertResponse {
    id: Option<String>,
}

pub fn clip_identity(path: &Path, metadata: &ClipMetadata) -> anyhow::Result<ClipIdentity> {
    let fs_metadata = fs::metadata(path).with_context(|| format!("reading metadata for {}", path.display()))?;
    let modified_unix_secs =
        fs_metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs()));
    let metadata_sha256 = sha256_hex(&serde_json::to_vec(metadata).context("serializing clip metadata")?);
    Ok(ClipIdentity {
        path: path.to_string_lossy().into_owned(),
        size_bytes: fs_metadata.len(),
        modified_unix_secs,
        metadata_sha256,
    })
}

pub fn render_youtube_metadata(settings: &AppSettings, path: &Path, metadata: &ClipMetadata) -> (String, String) {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("clip");
    let tokens = RunTemplateTokens::from_clip_metadata(stem, metadata);
    let title = tokens.render(&settings.youtube_title_template).trim().to_owned();
    let description = tokens.render(&settings.youtube_description_template);
    (if title.is_empty() { stem.to_owned() } else { title }, description)
}

pub struct UploadRequest {
    pub upload_id: String,
    pub path: PathBuf,
    pub title: String,
    pub description: String,
    pub visibility: YoutubeVisibility,
    pub metadata: ClipMetadata,
}

pub async fn upload_video(
    store: YoutubeUploadStore,
    req: UploadRequest,
    event_tx: tokio::sync::broadcast::Sender<crate::http::MonitorEvent>,
) {
    let UploadRequest { upload_id, path, title, description, visibility, metadata } = req;
    let result = upload_video_inner(&store, &upload_id, &path, &title, &description, visibility, &event_tx).await;
    match result {
        Ok(video_id) => {
            let video_url = format!("https://youtu.be/{video_id}");
            let status = store.update_upload(&upload_id, |status| {
                status.state = YoutubeUploadState::Uploaded;
                status.progress_bytes = status.total_bytes.unwrap_or(status.progress_bytes);
                status.progress_ratio = Some(1.0);
                status.video_id = Some(video_id.clone());
                status.video_url = Some(video_url.clone());
                status.error = None;
            });
            if let Ok(identity) = clip_identity(&path, &metadata) {
                let _ = store.append_history(UploadHistoryEntry {
                    identity,
                    video_id,
                    video_url,
                    uploaded_at: now_iso(),
                    title,
                });
            }
            if let Some(status) = status {
                let _ = event_tx.send(crate::http::MonitorEvent::YoutubeUploadChanged { upload: status });
            }
        }
        Err(err) => {
            tracing::warn!(path = %path.display(), "YouTube upload failed: {err:#}");
            if let Some(status) = store.update_upload(&upload_id, |status| {
                status.state = YoutubeUploadState::Failed;
                status.error = Some(format!("{err:#}"));
            }) {
                let _ = event_tx.send(crate::http::MonitorEvent::YoutubeUploadChanged { upload: status });
            }
        }
    }
}

async fn upload_video_inner(
    store: &YoutubeUploadStore,
    upload_id: &str,
    path: &Path,
    title: &str,
    description: &str,
    visibility: YoutubeVisibility,
    event_tx: &tokio::sync::broadcast::Sender<crate::http::MonitorEvent>,
) -> anyhow::Result<String> {
    let _permit = store.semaphore().acquire_owned().await.context("acquiring YouTube upload slot")?;
    let total_bytes = fs::metadata(path).with_context(|| format!("reading metadata for {}", path.display()))?.len();
    let access_token = store.access_token().await?;
    let config = store.config();
    publish_update(store, upload_id, event_tx, |status| {
        status.state = YoutubeUploadState::Uploading;
        status.progress_bytes = 0;
        status.progress_ratio = Some(0.0);
    });

    let client = reqwest::Client::new();
    let init_body = serde_json::json!({
        "snippet": { "title": title, "description": description },
        "status": { "privacyStatus": visibility.as_youtube_str() }
    });
    let init_url = format!("{}?uploadType=resumable&part=snippet,status", config.upload_url);
    let init = client
        .post(init_url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .header("X-Upload-Content-Type", "video/mp4")
        .header("X-Upload-Content-Length", total_bytes.to_string())
        .json(&init_body)
        .send()
        .await
        .context("starting YouTube resumable upload")?;
    let init_status = init.status();
    if !init_status.is_success() {
        anyhow::bail!("YouTube upload session failed with {init_status}: {}", init.text().await.unwrap_or_default());
    }
    let session_url = init
        .headers()
        .get(LOCATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| anyhow!("YouTube did not return an upload session URL"))?
        .to_owned();

    let mut file = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut uploaded = 0u64;
    let mut buffer = vec![0u8; CHUNK_SIZE as usize];
    loop {
        file.seek(SeekFrom::Start(uploaded))?;
        let remaining = total_bytes.saturating_sub(uploaded);
        if remaining == 0 {
            break;
        }
        let want = remaining.min(CHUNK_SIZE) as usize;
        let n = file.read(&mut buffer[..want])?;
        if n == 0 {
            break;
        }
        let start = uploaded;
        let end = uploaded + n as u64 - 1;
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, format!("Bearer {access_token}").parse()?);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("video/mp4"));
        headers.insert(CONTENT_LENGTH, n.to_string().parse()?);
        headers.insert(CONTENT_RANGE, format!("bytes {start}-{end}/{total_bytes}").parse()?);
        let response = client.put(&session_url).headers(headers).body(buffer[..n].to_vec()).send().await?;
        let status = response.status();
        if status == StatusCode::PERMANENT_REDIRECT || status.as_u16() == 308 {
            uploaded = uploaded_from_range(response.headers()).unwrap_or(end + 1);
            publish_progress(store, upload_id, event_tx, uploaded, total_bytes);
            continue;
        }
        if status.is_success() {
            publish_progress(store, upload_id, event_tx, total_bytes, total_bytes);
            let data: VideoInsertResponse = response.json().await.context("parsing YouTube upload response")?;
            let video_id = data.id.ok_or_else(|| anyhow!("YouTube upload response did not include a video ID"))?;
            publish_update(store, upload_id, event_tx, |status| status.state = YoutubeUploadState::Processing);
            return Ok(video_id);
        }
        anyhow::bail!("YouTube chunk upload failed with {status}: {}", response.text().await.unwrap_or_default());
    }

    anyhow::bail!("YouTube upload ended before a video response was returned")
}

fn publish_progress(
    store: &YoutubeUploadStore,
    upload_id: &str,
    event_tx: &tokio::sync::broadcast::Sender<crate::http::MonitorEvent>,
    uploaded: u64,
    total: u64,
) {
    publish_update(store, upload_id, event_tx, |status| {
        status.state = YoutubeUploadState::Uploading;
        status.progress_bytes = uploaded;
        status.total_bytes = Some(total);
        status.progress_ratio = (total > 0).then_some(uploaded as f64 / total as f64);
    });
}

fn publish_update(
    store: &YoutubeUploadStore,
    upload_id: &str,
    event_tx: &tokio::sync::broadcast::Sender<crate::http::MonitorEvent>,
    update: impl FnOnce(&mut YoutubeUploadStatus),
) {
    if let Some(status) = store.update_upload(upload_id, update) {
        let _ = event_tx.send(crate::http::MonitorEvent::YoutubeUploadChanged { upload: status });
    }
}

fn uploaded_from_range(headers: &HeaderMap) -> Option<u64> {
    let range = headers.get("Range")?.to_str().ok()?;
    let (_, end) = range.strip_prefix("bytes=0-")?.split_once('-').unwrap_or(("", range.strip_prefix("bytes=0-")?));
    end.parse::<u64>().ok().map(|n| n + 1)
}

fn upload_id(path: &Path, now: SystemTime) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update(unix_secs(now).to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn paths_match_for_current_platform(a: &str, b: &str) -> bool {
    let normalize = |path: &str| path.replace('\\', "/");
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        normalize(a).eq_ignore_ascii_case(&normalize(b))
    } else {
        normalize(a) == normalize(b)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn unix_secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_secs()
}

fn now_iso() -> String {
    crate::template_tokens::format_iso_utc(SystemTime::now())
}

fn youtube_client_secret() -> String {
    #[cfg(feature = "test-hooks")]
    if let Some(secret) = config::test_client_secret() {
        return secret;
    }
    obfstr::obfstring!(match option_env!("GE_YOUTUBE_CLIENT_SECRET") {
        Some(value) => value,
        None => "",
    })
}

impl YoutubeVisibility {
    fn as_youtube_str(self) -> &'static str {
        match self {
            YoutubeVisibility::Public => "public",
            YoutubeVisibility::Unlisted => "unlisted",
            YoutubeVisibility::Private => "private",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyring_store_round_trips_tokens() {
        let suffix = format!("{}-{}", std::process::id(), unix_secs(SystemTime::now()));
        let store = KeyringYoutubeCredentialStore::test_account(&suffix);
        let _ = store.delete();
        let tokens = YoutubeTokens {
            refresh_token: "refresh".to_owned(),
            access_token: Some("access".to_owned()),
            expires_at_unix_secs: Some(123),
            scope: Some(YOUTUBE_UPLOAD_SCOPE.to_owned()),
            token_type: Some("Bearer".to_owned()),
            account: Some(YoutubeAccount {
                email: Some("test@example.com".to_owned()),
                name: Some("Test User".to_owned()),
                picture: None,
            }),
        };
        store.save(&tokens).expect("save tokens");
        assert_eq!(store.load().expect("load tokens"), Some(tokens));
        store.delete().expect("delete tokens");
        assert_eq!(store.load().expect("load deleted tokens"), None);
    }

    #[test]
    fn fallback_store_uses_file_when_primary_fails() {
        let dir = crate::config::temp_dir().join(format!("ge-youtube-fallback-{}", std::process::id()));
        let path = dir.join("tokens.json");
        let _ = fs::remove_file(&path);
        let store = FallbackYoutubeCredentialStore {
            primary: Arc::new(FailingYoutubeCredentialStore),
            file: FileYoutubeCredentialStore { path: path.clone() },
        };
        let tokens = YoutubeTokens {
            refresh_token: "refresh".to_owned(),
            access_token: Some("access".to_owned()),
            expires_at_unix_secs: Some(123),
            scope: Some(YOUTUBE_UPLOAD_SCOPE.to_owned()),
            token_type: Some("Bearer".to_owned()),
            account: None,
        };

        store.save(&tokens).expect("save fallback tokens");
        assert_eq!(store.load().expect("load fallback tokens"), Some(tokens));
        store.delete().expect("delete fallback tokens");
        assert_eq!(store.load().expect("load deleted fallback tokens"), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn oauth_url_contains_required_parameters() {
        let config = YoutubeConfig {
            client_id: "client".to_owned(),
            client_secret: String::new(),
            auth_url: "https://example.test/auth".to_owned(),
            token_url: "https://example.test/token".to_owned(),
            upload_url: "https://example.test/upload".to_owned(),
            userinfo_url: "https://example.test/userinfo".to_owned(),
            redirect_uri: config::REDIRECT_URI.to_owned(),
            scope: YOUTUBE_UPLOAD_SCOPE.to_owned(),
            enabled: true,
        };
        let url = config.authorization_url("state-123");
        assert!(url.contains("client_id=client"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("state=state-123"));
        assert!(url.contains("youtube.upload"));
    }
}
