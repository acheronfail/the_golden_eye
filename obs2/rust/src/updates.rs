use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use reqwest::StatusCode;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http::AppState;
use crate::settings::UpdateCheckInterval;

const RELEASES_PAGE_URL: &str = "https://github.com/acheronfail/the_golden_eye/releases";
const RELEASE_URL_PREFIX: &str = "https://github.com/acheronfail/the_golden_eye/releases/";
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginUpdate {
    pub current_version: String,
    pub latest_version: String,
    pub release_url: String,
    pub updater_version: u32,
    pub requires_manual_install: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UpdatePhase {
    #[default]
    Idle,
    Checking,
    Available,
    Downloading,
    Staged,
    Applying,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub phase: UpdatePhase,
    pub available: Option<PluginUpdate>,
}

static CHECK_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

/// A release's downloadable file. Only the two fields `update_apply.rs`
/// needs to pick the right platform zip and its checksums.txt.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct GithubAsset {
    pub(crate) name: String,
    pub(crate) browser_download_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadUpdateResult {
    Staged,
    UpToDate,
    ManualInstallRequired,
}

pub async fn check_for_updates_on_startup(state: AppState) {
    // Dev builds restart the server on every hot reload, which would re-hit GitHub's
    // API each time (`last_update_check_time` only advances on success), so a
    // rate-limited dev session would keep retrying. No reason to check locally anyway.
    if cfg!(feature = "dev") {
        tracing::debug!("skipping plugin update check in a dev build");
        return;
    }
    if crate::update_apply::has_staged_update() {
        tracing::debug!("skipping plugin update check while an update is staged");
        return;
    }

    let settings = state.settings.get();
    if !is_check_due(settings.update_check_interval, settings.last_update_check_time, now_unix_seconds()) {
        tracing::debug!("plugin update check not due");
        return;
    }

    if let Err(err) = check_for_updates_now(state).await {
        tracing::warn!("plugin update check failed: {err:#}");
    }
}

/// Checks for an update now, bypassing the configured interval and dev skip. Shared by
/// the startup check and the manual "check now" endpoint. Records the check time, pushes
/// the retained app snapshot, and (if opted in) stages in the background. `Ok(None)` if up to date.
pub async fn check_for_updates_now(state: AppState) -> anyhow::Result<Option<PluginUpdate>> {
    let _check_guard = CHECK_LOCK.lock().await;
    let current = state.snapshot.current_update_status();
    if matches!(current.phase, UpdatePhase::Downloading | UpdatePhase::Staged | UpdatePhase::Applying) {
        return Ok(current.available);
    }
    state
        .snapshot
        .set_update_status(UpdateStatus { phase: UpdatePhase::Checking, available: current.available.clone() });

    if state.settings.status_without_runtime_defaults().file_error.is_some() {
        tracing::info!("settings file is invalid; skipping plugin update check");
        state.snapshot.set_update_status(UpdateStatus {
            phase: if current.available.is_some() { UpdatePhase::Available } else { UpdatePhase::Idle },
            available: current.available,
        });
        return Ok(None);
    }

    let checked_at = now_unix_seconds();
    let found = match fetch_latest_update(crate::PLUGIN_VERSION).await {
        Ok(found) => found,
        Err(err) => {
            let current = state.snapshot.current_update_status();
            state.snapshot.set_update_status(UpdateStatus {
                phase: if current.available.is_some() { UpdatePhase::Available } else { UpdatePhase::Idle },
                available: current.available,
            });
            return Err(err);
        }
    };
    if let Err(err) = state.settings.set_last_update_check_time(checked_at).context("saving last update check time") {
        let current = state.snapshot.current_update_status();
        state.snapshot.set_update_status(UpdateStatus {
            phase: if current.available.is_some() { UpdatePhase::Available } else { UpdatePhase::Idle },
            available: current.available,
        });
        return Err(err);
    }
    state.snapshot.set_settings_status(state.settings.status_without_runtime_defaults());

    let Some((update, assets)) = found else {
        tracing::info!(version = crate::PLUGIN_VERSION, "plugin is up to date");
        state.snapshot.set_update_status(UpdateStatus::default());
        return Ok(None);
    };

    tracing::info!(
        current_version = %update.current_version,
        latest_version = %update.latest_version,
        updater_version = update.updater_version,
        requires_manual_install = update.requires_manual_install,
        release_url = %update.release_url,
        "plugin update available"
    );
    let auto_update_enabled = state.settings.get().auto_update_enabled;
    state.snapshot.set_update_status(UpdateStatus {
        phase: if auto_update_enabled && !update.requires_manual_install {
            UpdatePhase::Downloading
        } else {
            UpdatePhase::Available
        },
        available: Some(update.clone()),
    });

    // Best-effort: this only feeds the "click to view the changelog" link on
    // the later "plugin updated" notice (see `routes::monitor::handle_socket`),
    // so a persistence failure here shouldn't fail the update check itself.
    if let Err(err) = state.settings.set_last_known_update(&update.latest_version, &update.release_url) {
        tracing::warn!("failed to persist last known plugin update: {err:#}");
    }

    // Only download/stage automatically when opted into auto installs. Otherwise the
    // "Download now" button / notice (via `download_and_stage_latest`) drives it on an
    // explicit click, so we don't fetch a release the user hasn't asked for.
    if auto_update_enabled && !update.requires_manual_install {
        // Reuses this same fetch's asset list rather than fetching the release
        // again, which would double GitHub API traffic for every check.
        let update_for_stage = update.clone();
        let state_for_stage = state.clone();
        let event_tx = state.event_tx.clone();
        tokio::spawn(async move {
            if let Err(err) = crate::update_apply::download_verify_and_stage(&update_for_stage, assets).await {
                tracing::error!("failed to stage plugin update: {err:#}");
                state_for_stage.snapshot.set_update_status(UpdateStatus {
                    phase: UpdatePhase::Available,
                    available: Some(update_for_stage),
                });
                let _ = event_tx.send(crate::http::AppEvent::UpdateStagingFailed { error: format!("{err:#}") });
            } else {
                state_for_stage
                    .snapshot
                    .set_update_status(UpdateStatus { phase: UpdatePhase::Staged, available: Some(update_for_stage) });
                if state_for_stage.settings.get().auto_update_enabled {
                    crate::update_apply::trigger_apply_if_safe(&state_for_stage);
                }
            }
        });
    }

    Ok(Some(update))
}

/// Fetches the latest release and, if compatible, downloads/verifies/stages it,
/// blocking until staging finishes. Explicit downloads bypass the auto-update
/// preference but never the updater-version compatibility gate.
pub async fn download_and_stage_latest(state: AppState) -> anyhow::Result<DownloadUpdateResult> {
    let previous = state.snapshot.current_update_status();
    if matches!(previous.phase, UpdatePhase::Staged | UpdatePhase::Applying) {
        return Ok(DownloadUpdateResult::Staged);
    }
    state
        .snapshot
        .set_update_status(UpdateStatus { phase: UpdatePhase::Downloading, available: previous.available.clone() });
    let found = match fetch_latest_update(crate::PLUGIN_VERSION).await {
        Ok(found) => found,
        Err(err) => {
            state.snapshot.set_update_status(UpdateStatus {
                phase: if previous.available.is_some() { UpdatePhase::Available } else { UpdatePhase::Idle },
                available: previous.available,
            });
            return Err(err);
        }
    };
    let Some((update, assets)) = found else {
        state.snapshot.set_update_status(UpdateStatus::default());
        return Ok(DownloadUpdateResult::UpToDate);
    };
    if update.requires_manual_install {
        state.snapshot.set_update_status(UpdateStatus { phase: UpdatePhase::Available, available: Some(update) });
        return Ok(DownloadUpdateResult::ManualInstallRequired);
    }
    state.snapshot.set_update_status(UpdateStatus { phase: UpdatePhase::Downloading, available: Some(update.clone()) });
    if let Err(err) = crate::update_apply::download_verify_and_stage(&update, assets).await {
        state.snapshot.set_update_status(UpdateStatus { phase: UpdatePhase::Available, available: Some(update) });
        return Err(err);
    }
    state.snapshot.set_update_status(UpdateStatus { phase: UpdatePhase::Staged, available: Some(update) });
    Ok(DownloadUpdateResult::Staged)
}

pub fn is_check_due(interval: UpdateCheckInterval, last_check_time: Option<u64>, now: u64) -> bool {
    let Some(interval_secs) = interval.interval_secs() else {
        return false;
    };
    let Some(last_check_time) = last_check_time else {
        return true;
    };
    now.saturating_sub(last_check_time) >= interval_secs
}

async fn fetch_latest_update(current_version: &str) -> anyhow::Result<Option<(PluginUpdate, Vec<GithubAsset>)>> {
    let client = reqwest::Client::builder().timeout(UPDATE_CHECK_TIMEOUT).build()?;
    let env_config = crate::config::UpdateEnvConfig::from_env();
    env_config.log();
    let releases_api_url = env_config.releases_api_url();
    let response = client
        .get(&releases_api_url)
        .header(reqwest::header::USER_AGENT, "the-golden-eye-obs-plugin")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .context("requesting latest GitHub release")?;

    if response.status() == StatusCode::NOT_FOUND {
        anyhow::bail!("latest GitHub release not found at {releases_api_url}");
    }

    let response = response.error_for_status().context("GitHub release API returned an error")?;
    let include_prereleases = env_config.include_prereleases();
    let body = response.json().await.context("parsing GitHub release response")?;
    let releases = releases_from_response(body)?;

    select_update_from_releases(current_version, releases, include_prereleases)
}

fn releases_from_response(value: Value) -> anyhow::Result<Vec<GithubRelease>> {
    if value.is_array() {
        Ok(serde_json::from_value(value).context("parsing GitHub releases")?)
    } else {
        Ok(vec![serde_json::from_value(value).context("parsing latest GitHub release")?])
    }
}

fn select_update_from_releases(
    current_version: &str,
    releases: Vec<GithubRelease>,
    include_prereleases: bool,
) -> anyhow::Result<Option<(PluginUpdate, Vec<GithubAsset>)>> {
    let current =
        parse_version(current_version).with_context(|| format!("parsing current version {current_version}"))?;
    let mut best: Option<(Version, GithubRelease)> = None;

    for release in releases {
        if release.draft || (!include_prereleases && release.prerelease) {
            continue;
        }

        let latest = parse_version(&release.tag_name)
            .with_context(|| format!("parsing latest release tag {}", release.tag_name))?;
        if latest <= current || best.as_ref().is_some_and(|(best_version, _)| latest <= *best_version) {
            continue;
        }

        best = Some((latest, release));
    }

    let Some((latest, release)) = best else {
        return Ok(None);
    };
    let updater_version =
        updater_version_from_assets(&latest, &release.assets, std::env::consts::OS, std::env::consts::ARCH)?;
    let installed_updater_version = installed_updater_version()?;
    let update = PluginUpdate {
        current_version: current_version.to_owned(),
        latest_version: release.tag_name,
        release_url: release.html_url,
        updater_version,
        requires_manual_install: updater_version != installed_updater_version,
    };
    Ok(Some((update, release.assets)))
}

fn parse_version(value: &str) -> anyhow::Result<Version> {
    let trimmed = value.trim().trim_start_matches('v');
    Ok(Version::parse(trimmed)?)
}

fn installed_updater_version() -> anyhow::Result<u32> {
    let version: u32 = crate::UPDATER_VERSION.parse().context("parsing installed updater version")?;
    if version == 0 {
        anyhow::bail!("installed updater version must be positive");
    }
    Ok(version)
}

pub(crate) fn platform_arch_suffix_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("macos-arm64"),
        ("macos", "x86_64") => Some("macos-x86_64"),
        ("windows", "x86_64") => Some("windows-x86_64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        _ => None,
    }
}

fn updater_version_from_assets(
    release_version: &Version,
    assets: &[GithubAsset],
    os: &str,
    arch: &str,
) -> anyhow::Result<u32> {
    let suffix = platform_arch_suffix_for(os, arch).context("unsupported OS/arch for auto-update")?;
    let name_suffix = format!("-{suffix}.zip");
    let candidates: Vec<&GithubAsset> = assets
        .iter()
        .filter(|asset| asset.name.starts_with("the_golden_eye-u") && asset.name.ends_with(&name_suffix))
        .collect();
    let [asset] = candidates.as_slice() else {
        anyhow::bail!("release must contain exactly one canonical package for {suffix}, found {}", candidates.len());
    };

    let middle = asset
        .name
        .strip_prefix("the_golden_eye-u")
        .and_then(|name| name.strip_suffix(&name_suffix))
        .context("canonical update package name is malformed")?;
    let (updater, version) = middle.split_once("-v").context("canonical update package name is missing '-v'")?;
    let updater: u32 = updater.parse().context("canonical update package has an invalid updater version")?;
    if updater == 0 {
        anyhow::bail!("canonical update package updater version must be positive");
    }
    let package_version = Version::parse(version).context("canonical update package has an invalid plugin version")?;
    if package_version != *release_version {
        anyhow::bail!("canonical update package version {package_version} does not match release {release_version}");
    }
    Ok(updater)
}

fn now_unix_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub fn open_release_url(url: &str) -> anyhow::Result<()> {
    if !is_allowed_release_url(url) {
        anyhow::bail!("refusing to open non-release URL: {url}");
    }
    crate::browser::open_url(url)
}

fn is_allowed_release_url(url: &str) -> bool {
    url == RELEASES_PAGE_URL || url.starts_with(RELEASE_URL_PREFIX)
}

#[cfg(test)]
#[path = "updates_test.rs"]
mod updates_test;
