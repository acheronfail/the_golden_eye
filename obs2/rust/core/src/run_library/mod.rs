//! Run-library operations: configured paths, history edits, imports, and catalog listing.
pub(crate) mod elite;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use anyhow::Context;
use ge_clip::{ClipMetadata, RomVersion, RunStatus};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::app::{AppEvent, SharedStateStore};
use crate::db::run_catalog::{
    IndexedRunClip,
    RunCatalog,
    RunCatalogRoot,
    RunCursor,
    RunListQuery,
    RunRecord,
    RunRetentionState,
    RunSort,
};
use crate::db::runs;
use crate::settings::AppSettings;

pub(crate) struct RunLibrary {
    catalog: Arc<RunCatalog>,
    needs_seed: Mutex<bool>,
    snapshot: SharedStateStore,
    event_tx: broadcast::Sender<AppEvent>,
}

impl RunLibrary {
    pub(crate) fn new(
        catalog: Arc<RunCatalog>,
        needs_seed: bool,
        snapshot: SharedStateStore,
        event_tx: broadcast::Sender<AppEvent>,
    ) -> Self {
        Self { catalog, needs_seed: Mutex::new(needs_seed), snapshot, event_tx }
    }

    pub(crate) fn seed_if_needed(&self, settings: &AppSettings) -> bool {
        let mut needs_seed = self.needs_seed.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if !*needs_seed {
            return false;
        }
        self.snapshot.set_run_catalog_sync(Some(RunCatalogSync::Initial));
        match seed_catalog_from_settings(&self.catalog, settings) {
            Ok(()) => *needs_seed = false,
            Err(error) => tracing::warn!("failed to seed run catalog: {error:#}"),
        }
        self.snapshot.set_run_catalog_sync(None);
        !*needs_seed
    }

    pub(crate) fn list(
        &self,
        settings: &AppSettings,
        params: RunsParams,
        cursor: Option<RunCursor>,
    ) -> anyhow::Result<RunsResponse> {
        let seeded = self.seed_if_needed(settings);
        if params.refresh && !seeded {
            self.snapshot.set_run_catalog_sync(Some(RunCatalogSync::Manual));
            let result = refresh_catalog_from_settings(&self.catalog, settings);
            self.snapshot.set_run_catalog_sync(None);
            result?;
        }
        let query = RunListQuery {
            sort: params.sort,
            cursor,
            limit: params.limit.unwrap_or(50),
            search: params.search,
            level_number: params.level.as_deref().and_then(crate::ge::level_info_by_name).map(|level| level.number),
            difficulty_number: params.difficulty.as_deref().and_then(crate::ge::difficulty_number),
            status: params.status,
            language: params.language,
            min_time_seconds: params.min_time_seconds,
            max_time_seconds: params.max_time_seconds,
        };
        let mut response = list_configured_run_page(settings, &self.catalog, &query);
        if let Some(run_id) = params.run_id
            && !response.clips.iter().any(|clip| clip.run_id == run_id)
            && let Some(run) = self.catalog.get_run(&run_id)?
        {
            response.requested_run = Some(run_clip_from_record(run));
        }
        Ok(response)
    }

    pub(crate) fn recent(&self, settings: &AppSettings, limit: usize) -> anyhow::Result<Vec<RunClip>> {
        self.seed_if_needed(settings);
        self.catalog.recent_runs(limit).map(|runs| runs.into_iter().map(run_clip_from_record).collect())
    }

    pub(crate) fn keep(&self, run_id: &str) -> anyhow::Result<RunClip> {
        let run = self.catalog.keep(run_id)?;
        self.publish_change(Some(run.run_id.clone()));
        Ok(run_clip_from_record(run))
    }

    pub(crate) fn delete(&self, run_id: &str, keep_history: bool) -> anyhow::Result<Option<RunClip>> {
        let retained = if keep_history {
            self.catalog.delete_video_keep_history(run_id).map(Some)
        } else {
            self.catalog.delete_run_and_video(run_id).map(|_| None)
        }?;
        self.publish_change(Some(run_id.to_owned()));
        Ok(retained.map(run_clip_from_record))
    }

    pub(crate) fn update_metadata(&self, req: RunMetadataUpdateRequest) -> Result<RunClip, RunPathError> {
        let clip = update_run_metadata(&self.catalog, req)?;
        self.publish_change(Some(clip.run_id.clone()));
        Ok(clip)
    }

    pub(crate) fn create_manual(&self, req: ManualRunRequest) -> Result<RunClip, RunPathError> {
        let clip = create_manual_run(&self.catalog, req)?;
        self.publish_change(Some(clip.run_id.clone()));
        Ok(clip)
    }

    pub(crate) fn import_elite(
        &self,
        username: &str,
        runs: Vec<crate::run_library::elite::EliteRun>,
    ) -> anyhow::Result<EliteImportResponse> {
        let result = import_elite_runs(&self.catalog, username, runs)?;
        self.publish_change(None);
        Ok(result)
    }

    fn publish_change(&self, run_id: Option<String>) {
        let _ = self.event_tx.send(AppEvent::RunCatalogChanged { run_id, save_id: None });
    }
}

#[derive(Debug, Deserialize)]
pub struct RunPathParams {
    pub(crate) path: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunsParams {
    #[serde(default)]
    pub(crate) refresh: bool,
    #[serde(default)]
    pub(crate) sort: RunSort,
    pub(crate) cursor: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) search: Option<String>,
    pub(crate) level: Option<String>,
    pub(crate) difficulty: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) min_time_seconds: Option<i32>,
    pub(crate) max_time_seconds: Option<i32>,
    pub(crate) run_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentRunsParams {
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunIdRequest {
    pub(crate) run_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDeleteRequest {
    pub(crate) run_id: String,
    pub(crate) keep_history: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRenameRequest {
    pub(crate) path: String,
    pub(crate) file_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunMetadataUpdateRequest {
    pub(crate) run_id: String,
    pub(crate) metadata: EditableRunMetadata,
}

#[derive(Debug, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ManualRunInput", rename_all = "camelCase")]
pub struct ManualRunRequest {
    pub(crate) date: String,
    pub(crate) level: String,
    pub(crate) difficulty: String,
    pub(crate) time: String,
    pub(crate) game_language: String,
    #[ts(optional)]
    pub(crate) rom_version: Option<RomVersion>,
    #[ts(optional)]
    pub(crate) youtube_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EliteImportRequest {
    pub(crate) username: String,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "TheEliteImportResponse", rename_all = "camelCase")]
pub struct EliteImportResponse {
    pub(crate) imported: usize,
    pub(crate) already_imported: usize,
    pub(crate) videos: usize,
}

#[derive(Debug, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct EditableRunMetadata {
    pub(crate) game_language: String,
    pub(crate) rom_version: Option<RomVersion>,
    pub(crate) status: String,
    pub(crate) difficulty: String,
    pub(crate) time: String,
    pub(crate) level: String,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RunsResponse {
    pub(crate) directories: Vec<RunDirectoryScan>,
    pub(crate) clips: Vec<RunClip>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) requested_run: Option<RunClip>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) total: Option<usize>,
    #[ts(optional = nullable)]
    pub(crate) next_cursor: Option<String>,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RunDirectoryScan {
    pub(crate) kind: RunDirectoryKind,
    pub(crate) path: String,
    pub(crate) exists: bool,
    #[ts(optional = nullable)]
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum RunDirectoryKind {
    Completed,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RunClip {
    pub(crate) run_id: String,
    pub(crate) path: String,
    pub(crate) file_name: String,
    pub(crate) directory: String,
    #[ts(type = "number")]
    pub(crate) size_bytes: u64,
    #[ts(optional = nullable)]
    pub(crate) modified: Option<String>,
    #[ts(optional = nullable)]
    pub(crate) duration_secs: Option<f64>,
    pub(crate) metadata: ClipMetadata,
    pub(crate) retention_state: RunRetentionState,
    pub(crate) retention_reason: Option<String>,
    #[ts(optional = nullable)]
    pub(crate) youtube: Option<crate::youtube_uploads::YoutubeMetadata>,
}

#[derive(Debug, Clone)]
pub(crate) struct ConfiguredRunDirectory {
    pub(crate) kind: RunDirectoryKind,
    pub(crate) path: PathBuf,
}

#[derive(Debug)]
pub(crate) enum RunPathError {
    BadRequest(&'static str),
    Conflict(&'static str),
    Forbidden(&'static str),
    NotFound(&'static str),
    Probe(anyhow::Error),
    Internal(anyhow::Error),
}

#[cfg(test)]
pub fn list_configured_runs(settings: &AppSettings, catalog: &RunCatalog, sort: RunSort) -> RunsResponse {
    list_configured_run_page(settings, catalog, &RunListQuery { sort, limit: 200, ..RunListQuery::default() })
}

pub fn list_configured_run_page(settings: &AppSettings, catalog: &RunCatalog, query: &RunListQuery) -> RunsResponse {
    let dirs = configured_run_directories(settings);
    let mut directories = Vec::new();

    for dir in &dirs {
        let display_path = dir.path.to_string_lossy().into_owned();
        match ensure_configured_run_directory(&dir.path) {
            Ok(()) => {
                directories.push(RunDirectoryScan { kind: dir.kind, path: display_path, exists: true, error: None })
            }
            Err(err) => directories.push(RunDirectoryScan {
                kind: dir.kind,
                path: display_path,
                exists: false,
                error: Some(err.to_string()),
            }),
        }
    }

    let (clips, total, next_cursor) = match catalog.list_run_page(query) {
        Ok(page) => (
            page.runs.into_iter().map(run_clip_from_record).collect(),
            page.total,
            page.next_cursor.map(|cursor| cursor.encode()),
        ),
        Err(err) => {
            tracing::warn!("failed to list run catalog: {err:#}");
            (Vec::new(), Some(0), None)
        }
    };

    RunsResponse { directories, clips, requested_run: None, total, next_cursor }
}

pub fn seed_catalog_from_settings(catalog: &RunCatalog, settings: &AppSettings) -> anyhow::Result<()> {
    refresh_catalog_from_settings(catalog, settings)
}

pub fn refresh_catalog_from_settings(catalog: &RunCatalog, settings: &AppSettings) -> anyhow::Result<()> {
    let dirs = configured_run_directories(settings);
    catalog.resync(&catalog_roots(&dirs))
}

pub(crate) fn ensure_configured_run_directory(dir: &Path) -> anyhow::Result<()> {
    runs::ensure_directory(dir)
}

pub fn tagged_clip(path: &Path) -> anyhow::Result<Option<RunClip>> {
    Ok(runs::read_from_disk(path)?.map(run_clip_from_indexed))
}

pub(crate) fn authorize_tagged_run_path(
    settings: &AppSettings,
    raw_path: &str,
) -> std::result::Result<PathBuf, RunPathError> {
    let requested = resolve_path(raw_path.trim());
    if raw_path.trim().is_empty() {
        return Err(RunPathError::BadRequest("path is required"));
    }
    if !is_video_file(&requested) {
        return Err(RunPathError::BadRequest("path is not a supported video file"));
    }

    let path = fs::canonicalize(&requested).map_err(|_| RunPathError::NotFound("run clip was not found"))?;
    if !configured_run_directories(settings)
        .into_iter()
        .filter_map(|dir| fs::canonicalize(dir.path).ok())
        .any(|dir| path.starts_with(dir))
    {
        return Err(RunPathError::Forbidden("run clip is not in a configured run directory"));
    }

    match ge_media::read_clip_metadata(&path) {
        Ok(Some(_)) => Ok(path),
        Ok(None) => Err(RunPathError::Forbidden("run clip was not created by The Golden Eye")),
        Err(err) => Err(RunPathError::Probe(err)),
    }
}

pub(crate) fn configured_run_directory_for_kind(
    settings: &AppSettings,
    kind: RunDirectoryKind,
) -> std::result::Result<PathBuf, RunPathError> {
    match kind {
        RunDirectoryKind::Completed => configured_dir(&settings.completed_output_path)
            .ok_or(RunPathError::NotFound("completed run clip folder is not configured")),
    }
}

pub(crate) fn rename_run_clip(
    settings: &AppSettings,
    catalog: &RunCatalog,
    req: RunRenameRequest,
) -> std::result::Result<RunClip, RunPathError> {
    let path = authorize_tagged_run_path(settings, &req.path)?;
    let file_name = normalized_run_file_name(&path, &req.file_name)?;
    let parent = path.parent().ok_or(RunPathError::BadRequest("run clip has no parent directory"))?;
    let target = parent.join(file_name);

    if target == path {
        return tagged_clip(&path)?.ok_or(RunPathError::Forbidden("run clip was not created by The Golden Eye"));
    }
    if target.exists() {
        return Err(RunPathError::Conflict("a run clip with that filename already exists"));
    }

    fs::rename(&path, &target).with_context(|| format!("renaming {} to {}", path.display(), target.display()))?;
    catalog.rename_path(&path, &target).map_err(RunPathError::Internal)?;
    tagged_clip(&target)?.ok_or(RunPathError::Forbidden("run clip was not created by The Golden Eye"))
}

fn update_run_metadata(
    catalog: &RunCatalog,
    req: RunMetadataUpdateRequest,
) -> std::result::Result<RunClip, RunPathError> {
    let mut metadata = catalog
        .get_run(&req.run_id)
        .map_err(RunPathError::Internal)?
        .ok_or(RunPathError::NotFound("run was not found"))?
        .metadata;
    apply_metadata_update(&mut metadata, req.metadata)?;
    catalog.update_metadata(&req.run_id, metadata).map(run_clip_from_record).map_err(RunPathError::Internal)
}

impl From<anyhow::Error> for RunPathError {
    fn from(err: anyhow::Error) -> Self {
        RunPathError::Internal(err)
    }
}

fn apply_metadata_update(
    metadata: &mut ClipMetadata,
    update: EditableRunMetadata,
) -> std::result::Result<(), RunPathError> {
    let level = normalize_level(&update.level)?;
    let time = normalize_time(&update.time)?;

    metadata.game_language = if let Some(rom_version) = update.rom_version {
        rom_version.game_language().to_owned()
    } else {
        normalize_game_language(&update.game_language)?.to_owned()
    };
    metadata.rom_version = update.rom_version;
    metadata.status = normalize_status(&update.status)?;
    metadata.difficulty = Some(normalize_difficulty(&update.difficulty)?.to_owned());
    metadata.level = level.name.to_owned();
    metadata.level_number = Some(level.number);
    metadata.time = time.as_ref().map(|(_, formatted)| formatted.clone());
    metadata.time_seconds = time.map(|(seconds, _)| seconds);

    Ok(())
}

fn normalize_game_language(value: &str) -> std::result::Result<&'static str, RunPathError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "en" => Ok("en"),
        "jp" => Ok("jp"),
        _ => Err(RunPathError::BadRequest("game language must be en or jp")),
    }
}

fn normalize_status(value: &str) -> std::result::Result<RunStatus, RunPathError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "complete" | "completed" => Ok(RunStatus::Complete),
        "failed" => Ok(RunStatus::Failed),
        "abort" | "aborted" => Ok(RunStatus::Abort),
        "kia" | "killed in action" => Ok(RunStatus::Kia),
        _ => Err(RunPathError::BadRequest("status must be failed, aborted, completed, or killed in action")),
    }
}

fn normalize_difficulty(value: &str) -> std::result::Result<&'static str, RunPathError> {
    crate::ge::difficulty_number(value)
        .and_then(crate::ge::difficulty_name)
        .ok_or(RunPathError::BadRequest("difficulty must be agent, secret agent, 00 agent, or 007"))
}

fn normalize_level(value: &str) -> std::result::Result<crate::ge::LevelInfo, RunPathError> {
    crate::ge::level_info_by_name(value)
        .ok_or(RunPathError::BadRequest("level must be one of the supported GoldenEye levels"))
}

fn normalize_time(value: &str) -> std::result::Result<Option<(i32, String)>, RunPathError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let Some((minutes, seconds)) = trimmed.split_once(':') else {
        return Err(RunPathError::BadRequest("time must be formatted as mm:ss"));
    };
    if minutes.is_empty() || seconds.len() != 2 || !minutes.chars().all(|c| c.is_ascii_digit()) {
        return Err(RunPathError::BadRequest("time must be formatted as mm:ss"));
    }
    let minutes = minutes.parse::<i32>().map_err(|_| RunPathError::BadRequest("time minutes are invalid"))?;
    let seconds = seconds.parse::<i32>().map_err(|_| RunPathError::BadRequest("time seconds are invalid"))?;
    if !(0..=59).contains(&seconds) {
        return Err(RunPathError::BadRequest("time seconds must be between 00 and 59"));
    }

    let total = minutes
        .checked_mul(60)
        .and_then(|m| m.checked_add(seconds))
        .ok_or(RunPathError::BadRequest("time is too large"))?;

    Ok(Some((total, format!("{minutes:02}:{seconds:02}"))))
}

fn create_manual_run(catalog: &RunCatalog, req: ManualRunRequest) -> std::result::Result<RunClip, RunPathError> {
    let level = crate::ge::level_info_by_name(&req.level).ok_or(RunPathError::BadRequest("select a valid level"))?;
    if crate::ge::difficulty_number(&req.difficulty).is_none() {
        return Err(RunPathError::BadRequest("select a valid difficulty"));
    }
    let game_language = if let Some(rom_version) = req.rom_version {
        rom_version.game_language().to_owned()
    } else {
        normalize_game_language(&req.game_language)?.to_owned()
    };
    let (time_seconds, time) = normalize_time(&req.time)?.ok_or(RunPathError::BadRequest("time is required"))?;
    let achieved = chrono::NaiveDate::parse_from_str(req.date.trim(), "%Y-%m-%d")
        .map_err(|_| RunPathError::BadRequest("enter a valid date"))?;
    let timestamp = achieved.and_hms_opt(12, 0, 0).unwrap().and_utc();
    let youtube = req
        .youtube_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|url| youtube_metadata(url, &format!("{} - {} - {time}", level.name, req.difficulty)))
        .transpose()?;
    let metadata = ClipMetadata {
        run_id: String::new(),
        timestamp: timestamp.to_rfc3339(),
        time: Some(time),
        time_seconds: Some(time_seconds),
        level: level.name.to_owned(),
        level_number: Some(level.number),
        difficulty: Some(req.difficulty),
        status: RunStatus::Complete,
        was_personal_best: false,
        game_language,
        rom_version: req.rom_version,
        source_name: "Manual entry".to_owned(),
        comment: "Added manually to run history".to_owned(),
        plugin_version: env!("GE_PLUGIN_VERSION").to_owned(),
        retention_state: "kept".to_owned(),
        retention_reason: Some("manualEntry".to_owned()),
    };
    let completed_at = std::time::SystemTime::from(timestamp);
    let (run, _) = catalog.create_history_run(completed_at, None, metadata, youtube).map_err(RunPathError::Internal)?;
    Ok(run_clip_from_record(run))
}

fn import_elite_runs(
    catalog: &RunCatalog,
    username: &str,
    elite_runs: Vec<crate::run_library::elite::EliteRun>,
) -> anyhow::Result<EliteImportResponse> {
    let mut response = EliteImportResponse { imported: 0, already_imported: 0, videos: 0 };
    for elite in elite_runs {
        let level =
            crate::ge::level_info_by_name(&elite.level).context("The Elite returned an unknown GoldenEye level")?;
        anyhow::ensure!(
            crate::ge::difficulty_number(&elite.difficulty).is_some(),
            "The Elite returned an unsupported difficulty"
        );
        let completed = chrono::DateTime::parse_from_rfc3339(&elite.timestamp)?;
        let source_url = format!("https://rankings.the-elite.net/~{username}/time/{}", elite.time_id);
        let youtube = elite.video_id.as_ref().map(|video_id| crate::youtube_uploads::YoutubeMetadata {
            video_id: video_id.clone(),
            video_url: format!("https://www.youtube.com/watch?v={video_id}"),
            uploaded_at: None,
            title: format!("{} - {} - {}", elite.level, elite.difficulty, elite.time),
            source: crate::youtube_uploads::YoutubeAssociationSource::TheElite,
        });
        let rom_version = elite_rom_version(&elite.system);
        let metadata = ClipMetadata {
            run_id: String::new(),
            timestamp: elite.timestamp.clone(),
            time: Some(elite.time),
            time_seconds: Some(elite.time_seconds),
            level: elite.level,
            level_number: Some(level.number),
            difficulty: Some(elite.difficulty),
            status: RunStatus::Complete,
            was_personal_best: elite.current_personal_best,
            game_language: rom_version.map(RomVersion::game_language).unwrap_or("").to_owned(),
            rom_version,
            source_name: format!("The Elite ({})", elite.system),
            comment: format!(
                "Imported from {source_url}; {}",
                if elite.current_personal_best { "current personal best" } else { "previous personal best" }
            ),
            plugin_version: env!("GE_PLUGIN_VERSION").to_owned(),
            retention_state: "kept".to_owned(),
            retention_reason: Some("theElite".to_owned()),
        };
        let requested_id = format!("the-elite-{}", elite.time_id);
        let (_, created) = catalog.create_history_run(
            std::time::SystemTime::from(completed),
            Some(&requested_id),
            metadata,
            youtube.clone(),
        )?;
        if created {
            response.imported += 1;
            response.videos += usize::from(youtube.is_some());
        } else {
            response.already_imported += 1;
        }
    }
    Ok(response)
}

fn elite_rom_version(system: &str) -> Option<RomVersion> {
    match system.trim().to_ascii_uppercase().as_str() {
        "NTSC" | "NTSC-U" => Some(RomVersion::NtscU),
        "NTSC-J" => Some(RomVersion::NtscJ),
        "PAL" => Some(RomVersion::Pal),
        _ => None,
    }
}

fn youtube_metadata(
    raw_url: &str,
    title: &str,
) -> std::result::Result<crate::youtube_uploads::YoutubeMetadata, RunPathError> {
    let url = reqwest::Url::parse(raw_url.trim()).map_err(|_| RunPathError::BadRequest("enter a valid YouTube URL"))?;
    let host = url.host_str().unwrap_or_default().trim_start_matches("www.");
    let video_id = match host {
        "youtu.be" => url.path_segments().and_then(|mut segments| segments.next()).map(str::to_owned),
        "youtube.com" | "m.youtube.com" => {
            if url.path() == "/watch" {
                url.query_pairs().find(|(key, _)| key == "v").map(|(_, value)| value.into_owned())
            } else {
                url.path_segments()
                    .and_then(|mut segments| (segments.next() == Some("embed")).then(|| segments.next()).flatten())
                    .map(str::to_owned)
            }
        }
        _ => None,
    }
    .filter(|value| {
        !value.is_empty()
            && value.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    })
    .ok_or(RunPathError::BadRequest("enter a valid YouTube video URL"))?;
    Ok(crate::youtube_uploads::YoutubeMetadata {
        video_url: format!("https://www.youtube.com/watch?v={video_id}"),
        video_id,
        uploaded_at: None,
        title: title.to_owned(),
        source: crate::youtube_uploads::YoutubeAssociationSource::ManualLink,
    })
}

fn normalized_run_file_name(path: &Path, raw: &str) -> std::result::Result<String, RunPathError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(RunPathError::BadRequest("filename is required"));
    }
    if trimmed == "." || trimmed == ".." || trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains('\0') {
        return Err(RunPathError::BadRequest("filename cannot contain path separators"));
    }

    let mut file_name = trimmed.to_owned();
    if Path::new(&file_name).extension().is_none()
        && let Some(ext) = path.extension().and_then(|ext| ext.to_str())
        && !ext.is_empty()
    {
        file_name.push('.');
        file_name.push_str(ext);
    }
    if !is_video_file(Path::new(&file_name)) {
        return Err(RunPathError::BadRequest("filename must use a supported video extension"));
    }

    Ok(file_name)
}

fn catalog_roots(dirs: &[ConfiguredRunDirectory]) -> Vec<RunCatalogRoot> {
    dirs.iter().map(|dir| RunCatalogRoot { path: dir.path.clone() }).collect()
}

fn run_clip_from_indexed(clip: IndexedRunClip) -> RunClip {
    let record = RunRecord {
        run_id: clip.run_id.clone(),
        metadata: clip.metadata.clone(),
        retention_state: clip.retention_state,
        retention_reason: clip.retention_reason.clone(),
        youtube: None,
        clip: Some(clip),
    };
    run_clip_from_record(record)
}

fn run_clip_from_record(run: RunRecord) -> RunClip {
    let path = run.clip.as_ref().map(|clip| clip.path.clone());
    RunClip {
        run_id: run.run_id,
        path: path.as_ref().map(|path| path.to_string_lossy().into_owned()).unwrap_or_default(),
        file_name: path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .unwrap_or_default(),
        directory: path
            .as_ref()
            .and_then(|path| path.parent())
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size_bytes: run.clip.as_ref().map(|clip| clip.size_bytes).unwrap_or_default(),
        modified: run.clip.as_ref().and_then(|clip| clip.modified).map(format_unix_timestamp),
        duration_secs: run.clip.as_ref().and_then(|clip| clip.duration_secs),
        metadata: run.metadata,
        retention_state: run.retention_state,
        retention_reason: run.retention_reason,
        youtube: run.youtube,
    }
}

fn configured_run_directories(settings: &AppSettings) -> Vec<ConfiguredRunDirectory> {
    let mut dirs = Vec::new();
    if let Some(path) = configured_dir(&settings.completed_output_path) {
        dirs.push(ConfiguredRunDirectory { kind: RunDirectoryKind::Completed, path });
    }
    dirs
}

fn configured_dir(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(resolve_path(trimmed))
}

fn resolve_path(path: &str) -> PathBuf {
    let expanded = expand_home(path);
    if expanded.is_absolute() { expanded } else { crate::config::current_dir().join(expanded) }
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~"
        && let Some(home) = crate::config::home_dir()
    {
        return home;
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = crate::config::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

fn is_video_file(path: &Path) -> bool {
    runs::is_video_file(path)
}

fn format_unix_timestamp(time: SystemTime) -> String {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(err) => format!("-{}", err.duration().as_secs()),
    }
}

#[cfg(test)]
#[path = "runs_test.rs"]
mod runs_test;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum RunCatalogSync {
    Initial,
    Manual,
}
