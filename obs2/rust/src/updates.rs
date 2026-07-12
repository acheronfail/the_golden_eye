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
}

pub async fn check_for_updates_on_startup(state: AppState) {
    if let Err(err) = check_for_updates_on_startup_inner(state).await {
        tracing::warn!("plugin update check failed: {err:#}");
    }
}

async fn check_for_updates_on_startup_inner(state: AppState) -> anyhow::Result<()> {
    let settings_status = state.settings.status();
    if settings_status.file_error.is_some() {
        tracing::info!("settings file is invalid; skipping plugin update check");
        return Ok(());
    }

    let settings = state.settings.get();
    if !is_check_due(settings.update_check_interval, settings.last_update_check_time, now_unix_seconds()) {
        tracing::debug!("plugin update check not due");
        return Ok(());
    }

    state.settings.set_last_update_check_time(now_unix_seconds()).context("saving last update check time")?;

    let Some(update) = fetch_latest_update(crate::PLUGIN_VERSION).await? else {
        tracing::info!(version = crate::PLUGIN_VERSION, "plugin is up to date");
        return Ok(());
    };

    tracing::info!(
        current_version = %update.current_version,
        latest_version = %update.latest_version,
        release_url = %update.release_url,
        "plugin update available"
    );
    state.update_tx.send_replace(Some(update));
    Ok(())
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

async fn fetch_latest_update(current_version: &str) -> anyhow::Result<Option<PluginUpdate>> {
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

    update_from_release(current_version, release)
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

fn releases_api_url() -> String {
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
mod tests {
    use super::*;

    #[test]
    fn due_check_respects_interval() {
        assert!(is_check_due(UpdateCheckInterval::Daily, None, 100));
        assert!(is_check_due(UpdateCheckInterval::Daily, Some(0), 24 * 60 * 60));
        assert!(!is_check_due(UpdateCheckInterval::Daily, Some(1), 24 * 60 * 60));
        assert!(!is_check_due(UpdateCheckInterval::Never, None, 100));
    }

    #[test]
    fn release_newer_than_current_is_reported() {
        let update = update_from_release(
            "1.2.3",
            GithubRelease {
                tag_name: "v1.3.0".to_owned(),
                html_url: "https://github.com/acheronfail/the_golden_eye/releases/tag/v1.3.0".to_owned(),
            },
        )
        .unwrap()
        .unwrap();

        assert_eq!(update.latest_version, "v1.3.0");
    }

    #[test]
    fn release_not_newer_than_current_is_ignored() {
        let update = update_from_release(
            "1.2.3",
            GithubRelease {
                tag_name: "v1.2.3".to_owned(),
                html_url: "https://github.com/acheronfail/the_golden_eye/releases/tag/v1.2.3".to_owned(),
            },
        )
        .unwrap();

        assert!(update.is_none());
    }

    #[test]
    fn release_url_validation_is_repo_scoped() {
        assert!(is_allowed_release_url("https://github.com/acheronfail/the_golden_eye/releases/tag/v1.2.3"));
        assert!(!is_allowed_release_url("https://github.com/acheronfail/other/releases/tag/v1.2.3"));
        assert!(!is_allowed_release_url("https://example.com/acheronfail/the_golden_eye/releases/tag/v1.2.3"));
    }
}
