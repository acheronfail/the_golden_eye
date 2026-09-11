mod credentials;
pub use credentials::YoutubeAccount;
use credentials::{YoutubeConfig, YoutubeCredentialStore, youtube_credential_store};
mod upload;
pub(crate) use upload::QueueError;
mod oauth;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
pub use ge_catalog::{UploadHistoryEntry, YoutubeAssociationSource, YoutubeMetadata};
use ge_clip::ClipMetadata;
pub(crate) use oauth::{CallbackError, ConnectError, DisconnectError, OAUTH_CALLBACK_PATH};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::db::run_catalog::RunCatalog;
use crate::settings::{AppSettings, SettingsStore};
use crate::template_tokens::RunTemplateTokens;

const UPLOAD_CONCURRENCY: usize = 2;
const USER_AGENT: &str = "the-golden-eye-obs-plugin";

#[derive(Clone)]
pub struct YoutubeUploadStore {
    inner: Arc<Mutex<YoutubeUploadInner>>,
    pending_oauth: Arc<tokio::sync::Mutex<Option<oauth::PendingOAuth>>>,
    semaphore: Arc<Semaphore>,
    run_catalog: Arc<RunCatalog>,
    credential_store: Arc<dyn YoutubeCredentialStore>,
    config: YoutubeConfig,
}

struct YoutubeUploadInner {
    uploads: HashMap<String, YoutubeUploadStatus>,
    path_to_active_id: HashMap<String, String>,
}

impl YoutubeUploadStore {
    pub fn new(settings_path: &Path, run_catalog: Arc<RunCatalog>) -> Self {
        Self::with_parts(run_catalog, youtube_credential_store(settings_path), YoutubeConfig::from_env())
    }

    pub fn with_parts(
        run_catalog: Arc<RunCatalog>,
        credential_store: Arc<dyn YoutubeCredentialStore>,
        config: YoutubeConfig,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(YoutubeUploadInner {
                uploads: HashMap::new(),
                path_to_active_id: HashMap::new(),
            })),
            pending_oauth: Arc::new(tokio::sync::Mutex::new(None)),
            semaphore: Arc::new(Semaphore::new(UPLOAD_CONCURRENCY)),
            run_catalog,
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
            history: self.read_history(),
        }
    }

    pub fn uploads(&self) -> Vec<YoutubeUploadStatus> {
        let mut uploads = self.inner.lock().unwrap().uploads.values().cloned().collect::<Vec<_>>();
        uploads.sort_by(|a, b| a.started_at.cmp(&b.started_at));
        uploads
    }

    pub fn active_upload_for_display_path(&self, display_path: &str) -> Option<YoutubeUploadStatus> {
        let inner = self.inner.lock().unwrap();
        let id = inner.path_to_active_id.get(display_path)?;
        inner.uploads.get(id).cloned()
    }

    pub fn insert_queued_upload(
        &self,
        upload_path: &Path,
        run_id: String,
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
            run_id,
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

    pub fn read_history(&self) -> Vec<UploadHistoryEntry> {
        self.run_catalog.youtube_history().unwrap_or_default()
    }

    pub fn set_history(&self, path: &Path, youtube: YoutubeMetadata) -> anyhow::Result<()> {
        self.run_catalog.set_youtube_history(path, &youtube)
    }

    pub(crate) fn forget(&self, settings: &SettingsStore, path: &str) -> Result<YoutubeStatus, ForgetError> {
        if !self.enabled() {
            return Err(ForgetError::Disabled);
        }
        let settings = settings.get_effective();
        crate::run_library::authorize_tagged_run_path(&settings, path).map_err(ForgetError::Path)?;
        self.forget_for_display_path(path.trim()).map_err(|err| {
            tracing::error!("failed to forget YouTube upload history: {err:#}");
            ForgetError::DeleteFailed
        })?;
        Ok(self.status())
    }

    pub fn forget_for_display_path(&self, display_path: &str) -> anyhow::Result<usize> {
        let removed_history = self.run_catalog.forget_youtube_history_for_display_path(display_path)?;
        let removed_uploads = self.forget_retained_uploads_for_display_path(display_path);
        Ok(removed_history + removed_uploads)
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

    pub fn semaphore(&self) -> Arc<Semaphore> {
        self.semaphore.clone()
    }

    pub fn config(&self) -> YoutubeConfig {
        self.config.clone()
    }
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "YouTubeStatus", rename_all = "camelCase")]
pub struct YoutubeStatus {
    pub enabled: bool,
    pub oauth_configured: bool,
    pub connected: bool,
    pub account: Option<YoutubeAccount>,
    pub uploads: Vec<YoutubeUploadStatus>,
    pub history: Vec<UploadHistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "YouTubeUploadStatus", rename_all = "camelCase")]
pub struct YoutubeUploadStatus {
    pub id: String,
    pub run_id: String,
    pub path: String,
    pub file_name: String,
    pub state: YoutubeUploadState,
    #[ts(type = "number")]
    pub progress_bytes: u64,
    #[ts(type = "number | null")]
    pub total_bytes: Option<u64>,
    pub progress_ratio: Option<f64>,
    pub video_id: Option<String>,
    pub video_url: Option<String>,
    pub error: Option<String>,
    pub title: String,
    #[serde(skip_serializing)]
    #[ts(skip)]
    pub description: String,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "YouTubeUploadState", rename_all = "camelCase")]
pub enum YoutubeUploadState {
    Queued,
    Uploading,
    Processing,
    Uploaded,
    Failed,
}

pub fn render_youtube_metadata(
    settings: &AppSettings,
    path: &Path,
    metadata: &ClipMetadata,
    datetime_local: Option<&str>,
) -> (String, String) {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("clip");
    let tokens = RunTemplateTokens::from_clip_metadata(stem, metadata);
    let datetime_local =
        datetime_local.map(str::trim).filter(|value| !value.is_empty()).unwrap_or(&tokens.timestamp_local);
    let title =
        tokens.render(&settings.youtube_title_template).replace("{datetime_local}", datetime_local).trim().to_owned();
    let description = tokens.render(&settings.youtube_description_template).replace("{datetime_local}", datetime_local);
    (if title.is_empty() { stem.to_owned() } else { title }, description)
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

fn unix_secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_secs()
}

fn now_iso() -> String {
    crate::template_tokens::format_iso_utc(SystemTime::now())
}

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(crate) enum ForgetError {
    Disabled,
    Path(crate::run_library::RunPathError),
    DeleteFailed,
}
