//! Upload workflow: validate and queue a clip, send chunks, publish progress, and retain its association.
use std::fs;
use std::future::Future;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, HeaderMap, HeaderValue, LOCATION};
use serde::Deserialize;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::{
    USER_AGENT,
    YoutubeAssociationSource,
    YoutubeMetadata,
    YoutubeUploadState,
    YoutubeUploadStatus,
    YoutubeUploadStore,
    now_iso,
    render_youtube_metadata,
};
use crate::app::AppEvent;
use crate::run_library::{RunPathError, authorize_tagged_run_path};
use crate::settings::{SettingsStore, YoutubeVisibility};

const UPLOAD_PROGRESS_EVENT_INTERVAL: Duration = Duration::from_millis(500);
const CHUNK_SIZE: u64 = 1024 * 1024;

#[derive(Debug)]
pub(crate) enum QueueError {
    Disabled,
    Disconnected,
    Path(RunPathError),
    Index,
    Metadata,
    File,
}

#[derive(Clone, Copy)]
pub(crate) enum UploadTrigger {
    Manual,
    PersonalBest,
}

impl UploadTrigger {
    fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::PersonalBest => "personal_best",
        }
    }
}

impl YoutubeUploadStore {
    pub(crate) fn queue_upload(
        &self,
        settings: &SettingsStore,
        path: &str,
        datetime_local: Option<&str>,
        trigger: UploadTrigger,
        event_tx: broadcast::Sender<AppEvent>,
    ) -> Result<(YoutubeUploadStatus, bool), QueueError> {
        if !self.enabled() {
            return Err(QueueError::Disabled);
        }
        if !self.connected() {
            return Err(QueueError::Disconnected);
        }
        let settings = settings.get_effective();
        let display_path = path.trim().to_owned();
        let path = authorize_tagged_run_path(&settings, &display_path).map_err(QueueError::Path)?;
        if let Some(existing) = self.active_upload_for_display_path(&display_path) {
            return Ok((existing, false));
        }
        let clip = self
            .run_catalog
            .refresh_clip(&path)
            .map_err(|err| {
                tracing::warn!(path = %path.display(), "failed to index clip before YouTube upload: {err:#}");
                QueueError::Index
            })?
            .ok_or(QueueError::Metadata)?;
        let total_bytes = fs::metadata(&path)
            .map_err(|err| {
                tracing::warn!(path = %path.display(), "failed to read clip file size: {err:#}");
                QueueError::File
            })?
            .len();
        let (title, description) = render_youtube_metadata(&settings, &path, &clip.metadata, datetime_local);
        let status = self.insert_queued_upload(
            &path,
            clip.run_id,
            display_path,
            title.clone(),
            description.clone(),
            total_bytes,
        );
        tracing::info!(
            upload_id = %status.id, run_id = %status.run_id, trigger = trigger.as_str(),
            "YouTube upload queued"
        );
        let _ = event_tx.send(AppEvent::YoutubeUploadChanged { upload: status.clone() });
        let store = self.clone();
        let request = UploadRequest {
            upload_id: status.id.clone(),
            run_id: status.run_id.clone(),
            path,
            title,
            description,
            visibility: settings.youtube_visibility,
        };
        let cancellation = self.cancellation(&status.id);
        tokio::spawn(async move {
            upload_video(store, request, event_tx, cancellation).await;
        });
        Ok((status, true))
    }
}

#[derive(Debug, Deserialize)]
struct VideoInsertResponse {
    id: Option<String>,
}

pub struct UploadRequest {
    pub upload_id: String,
    pub run_id: String,
    pub path: PathBuf,
    pub title: String,
    pub description: String,
    pub visibility: YoutubeVisibility,
}

#[tracing::instrument(skip_all, fields(upload_id = %req.upload_id, run_id = %req.run_id, stage = "queued"))]
pub async fn upload_video(
    store: YoutubeUploadStore,
    req: UploadRequest,
    event_tx: tokio::sync::broadcast::Sender<crate::app::AppEvent>,
    cancellation: CancellationToken,
) {
    let started = Instant::now();
    let result = upload_video_inner(&store, &req, &event_tx, &cancellation).await;
    let UploadRequest { upload_id, path, title, .. } = req;
    match result {
        Ok(video_id) => {
            let video_url = format!("https://youtu.be/{video_id}");
            let status = store.update_upload(&upload_id, |status| {
                let message = if status.state == YoutubeUploadState::Cancelling {
                    "YouTube upload completed before cancellation"
                } else {
                    "YouTube upload completed"
                };
                tracing::info!(%video_id, elapsed_secs = started.elapsed().as_secs_f64(), "{message}");
                status.state = YoutubeUploadState::Uploaded;
                status.progress_bytes = status.total_bytes.unwrap_or(status.progress_bytes);
                status.progress_ratio = Some(1.0);
                status.video_id = Some(video_id.clone());
                status.video_url = Some(video_url.clone());
                status.error = None;
            });
            if let Err(error) = store.set_history(
                &path,
                YoutubeMetadata {
                    video_id: video_id.clone(),
                    video_url,
                    uploaded_at: Some(now_iso()),
                    title,
                    source: YoutubeAssociationSource::PluginUpload,
                },
            ) {
                tracing::warn!(%video_id, error = %format!("{error:#}"), "failed to persist completed YouTube upload association");
            }
            if let Some(status) = status {
                let _ = event_tx.send(crate::app::AppEvent::YoutubeUploadChanged { upload: status });
            }
        }
        Err(err) if err.is::<UploadCancelled>() => {
            tracing::info!(elapsed_secs = started.elapsed().as_secs_f64(), "YouTube upload cancelled");
            publish_update(&store, &upload_id, &event_tx, |status| {
                status.state = YoutubeUploadState::Cancelled;
                status.error = None;
            });
        }
        Err(err) => {
            tracing::warn!(path = %path.display(), elapsed_secs = started.elapsed().as_secs_f64(), "YouTube upload failed: {err:#}");
            if let Some(status) = store.update_upload(&upload_id, |status| {
                status.state = YoutubeUploadState::Failed;
                status.error = Some(format!("{err:#}"));
            }) {
                let _ = event_tx.send(crate::app::AppEvent::YoutubeUploadChanged { upload: status });
            }
        }
    }
}

async fn upload_video_inner(
    store: &YoutubeUploadStore,
    req: &UploadRequest,
    event_tx: &tokio::sync::broadcast::Sender<crate::app::AppEvent>,
    cancellation: &CancellationToken,
) -> anyhow::Result<String> {
    let UploadRequest { upload_id, path, title, description, visibility, .. } = req;
    let _permit =
        cancel_or(cancellation, store.semaphore().acquire_owned()).await.context("acquiring YouTube upload slot")?;
    tracing::Span::current().record("stage", "preparing");
    let total_bytes = fs::metadata(path).with_context(|| format!("reading metadata for {}", path.display()))?.len();
    tracing::info!(total_bytes, visibility = visibility.as_youtube_str(), "YouTube upload started");
    tracing::Span::current().record("stage", "authentication");
    let access_token = cancel_or(cancellation, store.access_token()).await?;
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
    tracing::Span::current().record("stage", "session");
    let init_request = client
        .post(init_url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .header("X-Upload-Content-Type", "video/mp4")
        .header("X-Upload-Content-Length", total_bytes.to_string())
        .json(&init_body)
        .send();
    let init = cancel_or(cancellation, init_request).await.context("starting YouTube resumable upload")?;
    let init_status = init.status();
    if !init_status.is_success() {
        anyhow::bail!(
            "YouTube upload session failed with {init_status}: {}",
            cancel_or(cancellation, init.text()).await?
        );
    }
    let session_url = init
        .headers()
        .get(LOCATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| anyhow!("YouTube did not return an upload session URL"))?
        .to_owned();

    let mut file = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut uploaded = 0u64;
    let mut progress_publisher = ProgressPublisher::new();
    let mut buffer = vec![0u8; CHUNK_SIZE as usize];
    tracing::Span::current().record("stage", "transfer");
    loop {
        if cancellation.is_cancelled() {
            return Err(UploadCancelled.into());
        }
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
        let request = client.put(&session_url).headers(headers).body(buffer[..n].to_vec());
        if end + 1 == total_bytes {
            tracing::Span::current().record("stage", "final_chunk");
            match finish_upload(request, &client, &session_url, &access_token, total_bytes, cancellation).await? {
                FinalChunk::Uploaded(video_id) => return Ok(video_id),
                FinalChunk::Incomplete(bytes) => {
                    tracing::Span::current().record("stage", "transfer");
                    uploaded = bytes;
                    progress_publisher.update(store, upload_id, event_tx, uploaded, total_bytes);
                    continue;
                }
            }
        }
        let response = cancel_or(cancellation, request.send()).await?;
        let status = response.status();
        if status == StatusCode::PERMANENT_REDIRECT || status.as_u16() == 308 {
            uploaded = uploaded_from_range(response.headers()).unwrap_or(end + 1);
            progress_publisher.update(store, upload_id, event_tx, uploaded, total_bytes);
            continue;
        }
        if status.is_success() {
            progress_publisher.publish_now(store, upload_id, event_tx, total_bytes, total_bytes);
            let data: VideoInsertResponse =
                cancel_or(cancellation, response.json()).await.context("parsing YouTube upload response")?;
            let video_id = data.id.ok_or_else(|| anyhow!("YouTube upload response did not include a video ID"))?;
            publish_update(store, upload_id, event_tx, |status| status.state = YoutubeUploadState::Processing);
            return Ok(video_id);
        }
        anyhow::bail!("YouTube chunk upload failed with {status}: {}", cancel_or(cancellation, response.text()).await?);
    }

    anyhow::bail!("YouTube upload ended before a video response was returned")
}

#[derive(Debug)]
struct UploadCancelled;

impl std::fmt::Display for UploadCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Upload cancelled")
    }
}

impl std::error::Error for UploadCancelled {}

async fn cancel_or<T, E: Into<anyhow::Error>>(
    cancellation: &CancellationToken,
    future: impl Future<Output = Result<T, E>>,
) -> anyhow::Result<T> {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(UploadCancelled.into()),
        result = future => result.map_err(Into::into),
    }
}

async fn video_response(response: reqwest::Response) -> anyhow::Result<String> {
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("YouTube upload completion returned {status}");
    }
    let data: VideoInsertResponse = response.json().await.context("parsing YouTube upload response")?;
    data.id.ok_or_else(|| anyhow!("YouTube upload response did not include a video ID"))
}

enum FinalChunk {
    Uploaded(String),
    Incomplete(u64),
}

async fn finish_upload(
    request: reqwest::RequestBuilder,
    client: &reqwest::Client,
    session_url: &str,
    access_token: &str,
    total_bytes: u64,
    cancellation: &CancellationToken,
) -> anyhow::Result<FinalChunk> {
    if cancellation.is_cancelled() {
        return Err(UploadCancelled.into());
    }
    let result = {
        let completion = async {
            let response = request.send().await?;
            if response.status() == StatusCode::PERMANENT_REDIRECT {
                return Ok(FinalChunk::Incomplete(uploaded_from_range(response.headers()).unwrap_or(0)));
            }
            video_response(response).await.map(FinalChunk::Uploaded)
        };
        tokio::pin!(completion);
        tokio::select! {
            biased;
            result = &mut completion => result,
            _ = cancellation.cancelled() => {
                tracing::debug!("cancellation requested during final chunk; waiting for YouTube completion");
                // Keep the final request alive briefly so its response can settle the completion race.
                tokio::time::timeout(Duration::from_secs(10), &mut completion)
                    .await.unwrap_or_else(|_| Err(anyhow!("Timed out waiting for upload completion")))
            }
        }
    };
    if cancellation.is_cancelled() && matches!(result, Ok(FinalChunk::Incomplete(_))) {
        tracing::debug!("final chunk remained incomplete after cancellation");
        return Err(UploadCancelled.into());
    }
    if result.is_ok() || !cancellation.is_cancelled() {
        return result;
    }
    tracing::Span::current().record("stage", "completion_probe");
    tracing::debug!("checking YouTube completion after cancellation");
    let check = async {
        let response = client
            .put(session_url)
            .header(AUTHORIZATION, format!("Bearer {access_token}"))
            .header(CONTENT_LENGTH, "0")
            .header(CONTENT_RANGE, format!("bytes */{total_bytes}"))
            .send()
            .await?;
        video_response(response).await
    };
    // An incomplete status can race server-side finalization, so only a video ID settles this case.
    let verification = tokio::time::timeout(Duration::from_secs(10), check)
        .await
        .map_err(anyhow::Error::from)
        .and_then(|result| result);
    match &verification {
        Ok(video_id) => tracing::debug!(%video_id, "YouTube completion verified after cancellation"),
        Err(_) => tracing::debug!("YouTube completion probe failed"),
    }
    verification.map(FinalChunk::Uploaded).context(
        "Transfer stopped, but YouTube completion could not be verified. Check YouTube Studio before uploading again",
    )
}

struct ProgressPublisher {
    last_event_at: Option<Instant>,
}

impl ProgressPublisher {
    fn new() -> Self {
        Self { last_event_at: None }
    }

    fn update(
        &mut self,
        store: &YoutubeUploadStore,
        upload_id: &str,
        event_tx: &tokio::sync::broadcast::Sender<crate::app::AppEvent>,
        uploaded: u64,
        total: u64,
    ) {
        let status = update_progress(store, upload_id, uploaded, total);
        let now = Instant::now();
        let should_publish = self
            .last_event_at
            .is_none_or(|last_event_at| now.duration_since(last_event_at) >= UPLOAD_PROGRESS_EVENT_INTERVAL);
        if should_publish {
            self.last_event_at = Some(now);
            publish_status(event_tx, status);
        }
    }

    fn publish_now(
        &mut self,
        store: &YoutubeUploadStore,
        upload_id: &str,
        event_tx: &tokio::sync::broadcast::Sender<crate::app::AppEvent>,
        uploaded: u64,
        total: u64,
    ) {
        self.last_event_at = Some(Instant::now());
        publish_status(event_tx, update_progress(store, upload_id, uploaded, total));
    }
}

fn update_progress(
    store: &YoutubeUploadStore,
    upload_id: &str,
    uploaded: u64,
    total: u64,
) -> Option<YoutubeUploadStatus> {
    store.update_upload(upload_id, |status| {
        status.state = YoutubeUploadState::Uploading;
        status.progress_bytes = uploaded;
        status.total_bytes = Some(total);
        status.progress_ratio = (total > 0).then_some(uploaded as f64 / total as f64);
    })
}

fn publish_status(
    event_tx: &tokio::sync::broadcast::Sender<crate::app::AppEvent>,
    status: Option<YoutubeUploadStatus>,
) {
    if let Some(upload) = status {
        let _ = event_tx.send(crate::app::AppEvent::YoutubeUploadChanged { upload });
    }
}

fn publish_update(
    store: &YoutubeUploadStore,
    upload_id: &str,
    event_tx: &tokio::sync::broadcast::Sender<crate::app::AppEvent>,
    update: impl FnOnce(&mut YoutubeUploadStatus),
) {
    if let Some(status) = store.update_upload(upload_id, update) {
        let _ = event_tx.send(crate::app::AppEvent::YoutubeUploadChanged { upload: status });
    }
}

fn uploaded_from_range(headers: &HeaderMap) -> Option<u64> {
    let range = headers.get("Range")?.to_str().ok()?;
    let (_, end) = range.strip_prefix("bytes=0-")?.split_once('-').unwrap_or(("", range.strip_prefix("bytes=0-")?));
    end.parse::<u64>().ok().map(|n| n + 1)
}
