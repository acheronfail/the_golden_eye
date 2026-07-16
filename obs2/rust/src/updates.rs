use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use reqwest::StatusCode;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::http::AppState;
use crate::settings::UpdateCheckInterval;

const RELEASES_API_URL: &str = "https://api.github.com/repos/acheronfail/the_golden_eye/releases/latest";
const RELEASES_PAGE_URL: &str = "https://github.com/acheronfail/the_golden_eye/releases";
const RELEASE_URL_PREFIX: &str = "https://github.com/acheronfail/the_golden_eye/releases/";
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(10);
const UPDATE_CHECK_URL_ENV: &str = "GE_UPDATE_CHECK_URL";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginUpdate {
    pub current_version: String,
    pub latest_version: String,
    pub release_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
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

pub async fn check_for_updates_on_startup(state: AppState) {
    // Dev builds restart the server on every hot reload, which would re-hit GitHub's
    // API each time (`last_update_check_time` only advances on success), so a
    // rate-limited dev session would keep retrying. No reason to check locally anyway.
    if cfg!(feature = "dev") {
        tracing::debug!("skipping plugin update check in a dev build");
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
    if state.settings.status_without_runtime_defaults().file_error.is_some() {
        tracing::info!("settings file is invalid; skipping plugin update check");
        return Ok(None);
    }

    let checked_at = now_unix_seconds();
    let found = fetch_latest_update(crate::PLUGIN_VERSION).await?;
    state.settings.set_last_update_check_time(checked_at).context("saving last update check time")?;
    state.snapshot.set_settings_status(state.settings.status_without_runtime_defaults());

    let Some((update, assets)) = found else {
        tracing::info!(version = crate::PLUGIN_VERSION, "plugin is up to date");
        state.snapshot.set_update(None);
        return Ok(None);
    };

    tracing::info!(
        current_version = %update.current_version,
        latest_version = %update.latest_version,
        release_url = %update.release_url,
        "plugin update available"
    );
    state.snapshot.set_update(Some(update.clone()));

    // Best-effort: this only feeds the "click to view the changelog" link on
    // the later "plugin updated" notice (see `routes::monitor::handle_socket`),
    // so a persistence failure here shouldn't fail the update check itself.
    if let Err(err) = state.settings.set_last_known_update(&update.latest_version, &update.release_url) {
        tracing::warn!("failed to persist last known plugin update: {err:#}");
    }

    // Only download/stage automatically when opted into auto installs. Otherwise the
    // "Download now" button / notice (via `download_and_stage_latest`) drives it on an
    // explicit click, so we don't fetch a release the user hasn't asked for.
    if state.settings.get().auto_update_enabled {
        // Reuses this same fetch's asset list rather than fetching the release
        // again, which would double GitHub API traffic for every check.
        let update_for_stage = update.clone();
        let event_tx = state.event_tx.clone();
        tokio::spawn(async move {
            if let Err(err) = crate::update_apply::download_verify_and_stage(&update_for_stage, assets).await {
                tracing::error!("failed to stage plugin update: {err:#}");
                let _ = event_tx.send(crate::http::MonitorEvent::UpdateStagingFailed { error: format!("{err:#}") });
            }
        });
    }

    Ok(Some(update))
}

/// Fetches the latest release and, if newer, downloads/verifies/stages it, blocking
/// until staging finishes. The explicit-download path (behind "Download now"/notice),
/// runs regardless of auto-update. Returns whether staged; then apply can install it.
pub async fn download_and_stage_latest() -> anyhow::Result<bool> {
    let Some((update, assets)) = fetch_latest_update(crate::PLUGIN_VERSION).await? else {
        return Ok(false);
    };
    crate::update_apply::download_verify_and_stage(&update, assets).await?;
    Ok(true)
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
    let releases_api_url = releases_api_url();
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
    let release: GithubRelease = response.json().await.context("parsing latest GitHub release")?;
    let assets = release.assets.clone();

    Ok(update_from_release(current_version, release)?.map(|update| (update, assets)))
}

fn update_from_release(current_version: &str, release: GithubRelease) -> anyhow::Result<Option<PluginUpdate>> {
    let current =
        parse_version(current_version).with_context(|| format!("parsing current version {current_version}"))?;
    let latest =
        parse_version(&release.tag_name).with_context(|| format!("parsing latest release tag {}", release.tag_name))?;

    if latest <= current {
        return Ok(None);
    }

    Ok(Some(PluginUpdate {
        current_version: current_version.to_owned(),
        latest_version: release.tag_name,
        release_url: release.html_url,
    }))
}

fn parse_version(value: &str) -> anyhow::Result<Version> {
    let trimmed = value.trim().trim_start_matches('v');
    Ok(Version::parse(trimmed)?)
}

pub(crate) fn releases_api_url() -> String {
    std::env::var(UPDATE_CHECK_URL_ENV).unwrap_or_else(|_| RELEASES_API_URL.to_owned())
}

fn now_unix_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub fn open_release_url(url: &str) -> anyhow::Result<()> {
    if !is_allowed_release_url(url) {
        anyhow::bail!("refusing to open non-release URL: {url}");
    }

    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(url).status();

    #[cfg(target_os = "windows")]
    let status = Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).status();

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let status = Command::new("xdg-open").arg(url).status();

    let status = status?;
    if status.success() { Ok(()) } else { anyhow::bail!("browser opener exited with status {status}") }
}

fn is_allowed_release_url(url: &str) -> bool {
    url == RELEASES_PAGE_URL || url.starts_with(RELEASE_URL_PREFIX)
}

#[cfg(test)]
#[path = "updates_test.rs"]
mod updates_test;
