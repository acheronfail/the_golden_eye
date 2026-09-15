//! OAuth configuration and token persistence, including the keyring/file fallback.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;
use std::time::SystemTime;

use anyhow::Context;
use keyring::Entry;
use serde::{Deserialize, Serialize};

use super::unix_secs;
use crate::config;

const YOUTUBE_UPLOAD_SCOPE: &str = "openid email profile https://www.googleapis.com/auth/youtube.upload";
const KEYRING_SERVICE: &str = "the-golden-eye.youtube";
const KEYRING_ACCOUNT: &str = "oauth-tokens";
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
        let client_secret = config::client_secret();
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "YouTubeAccount", rename_all = "camelCase")]
pub struct YoutubeAccount {
    pub email: Option<String>,
    pub name: Option<String>,
    pub picture: Option<String>,
}

impl YoutubeTokens {
    pub(super) fn access_token_valid(&self) -> Option<&str> {
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
        Err(anyhow::anyhow!("keyring unavailable"))
    }

    fn save(&self, _tokens: &YoutubeTokens) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("keyring unavailable"))
    }

    fn delete(&self) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("keyring unavailable"))
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

pub(super) fn youtube_credential_store(settings_path: &Path) -> Arc<dyn YoutubeCredentialStore> {
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

#[cfg(test)]
#[path = "credentials_test.rs"]
mod tests;
