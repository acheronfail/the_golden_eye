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
    // Dev builds (`just dev`) restart the server on every hot reload, each of
    // which would otherwise re-hit GitHub's release API: `last_update_check_time`
    // only advances on a *successful* check (see `check_for_updates_now`, and
    // the `failed_startup_update_check_does_not_persist_check_time` test), so
    // once a dev session gets rate-limited it would retry -- and get
    // rate-limited again -- on every single reload thereafter. There's also no
    // reason to check for updates while iterating locally.
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

/// Checks for an update right now, unconditionally -- bypassing the
/// configured interval (and the dev-build skip above, which only guards the
/// *automatic* startup check). Shared by that startup check and the manual
/// "check now" endpoint (`POST /api/v1/updates/check`), so a user isn't
/// stuck waiting out the interval just because an earlier automatic check
/// already ran this week.
///
/// Records the check time and pushes `UpdateAvailable` over the WebSocket if a
/// newer release exists. When the user has opted into automatic installs it
/// also kicks off staging in the background; without that opt-in the download
/// waits for an explicit request (see `download_and_stage_latest`) so we never
/// pull down a release the user hasn't asked for. Staging alone never touches
/// the running plugin -- only applying it, gated separately, does. Returns
/// `Ok(None)` both when the plugin is already up to date and when the settings
/// file is currently invalid (there's nowhere durable to record the check
/// either way).
pub async fn check_for_updates_now(state: AppState) -> anyhow::Result<Option<PluginUpdate>> {
    if state.settings.status().file_error.is_some() {
        tracing::info!("settings file is invalid; skipping plugin update check");
        return Ok(None);
    }

    let checked_at = now_unix_seconds();
    let found = fetch_latest_update(crate::PLUGIN_VERSION).await?;
    state.settings.set_last_update_check_time(checked_at).context("saving last update check time")?;

    let Some((update, assets)) = found else {
        tracing::info!(version = crate::PLUGIN_VERSION, "plugin is up to date");
        return Ok(None);
    };

    tracing::info!(
        current_version = %update.current_version,
        latest_version = %update.latest_version,
        release_url = %update.release_url,
        "plugin update available"
    );
    state.update_tx.send_replace(Some(update.clone()));

    // Best-effort: this only feeds the "click to view the changelog" link on
    // the later "plugin updated" notice (see `routes::monitor::handle_socket`),
    // so a persistence failure here shouldn't fail the update check itself.
    if let Err(err) = state.settings.set_last_known_update(&update.latest_version, &update.release_url) {
        tracing::warn!("failed to persist last known plugin update: {err:#}");
    }

    // Only download and stage automatically when the user opted into auto
    // installs. Otherwise the "Download now" button / "download and install"
    // notice (both via `download_and_stage_latest`) drive the download on an
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

/// Fetches the latest release and, if it's newer than what's running,
/// downloads, verifies, and stages it -- blocking until staging finishes (or
/// fails). Unlike the background staging `check_for_updates_now` kicks off when
/// auto-update is enabled, this is the explicit-download path behind the
/// "Download now" button and the "download and install" notice, so it runs
/// regardless of the auto-update setting. Returns whether an update was staged
/// (`false` means the plugin is already up to date). Once this returns `true`,
/// `POST /api/v1/updates/apply` can install it.
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
                assets: Vec::new(),
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
                assets: Vec::new(),
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
