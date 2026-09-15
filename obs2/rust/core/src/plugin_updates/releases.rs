use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use reqwest::StatusCode;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::settings::UpdateCheckInterval;

const RELEASES_PAGE_URL: &str = "https://github.com/acheronfail/the_golden_eye/releases";
const RELEASE_URL_PREFIX: &str = "https://github.com/acheronfail/the_golden_eye/releases/";
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, PartialEq, Eq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct PluginUpdate {
    pub current_version: String,
    pub latest_version: String,
    pub release_url: String,
    pub updater_version: u32,
    pub requires_manual_install: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum UpdatePhase {
    #[default]
    Idle,
    Checking,
    Available,
    Downloading,
    Staged,
    Applying,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub phase: UpdatePhase,
    pub available: Option<PluginUpdate>,
}

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

/// A release's downloadable file. Only the two fields `installation.rs`
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

pub fn is_check_due(interval: UpdateCheckInterval, last_check_time: Option<u64>, now: u64) -> bool {
    let Some(interval_secs) = interval.interval_secs() else {
        return false;
    };
    let Some(last_check_time) = last_check_time else {
        return true;
    };
    now.saturating_sub(last_check_time) >= interval_secs
}

pub(super) async fn fetch_latest_update(
    current_version: &str,
) -> anyhow::Result<Option<(PluginUpdate, Vec<GithubAsset>)>> {
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
    crate::UPDATER_VERSION.parse().context("parsing installed updater version")
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
    let package_version = Version::parse(version).context("canonical update package has an invalid plugin version")?;
    if package_version != *release_version {
        anyhow::bail!("canonical update package version {package_version} does not match release {release_version}");
    }
    Ok(updater)
}

pub(super) fn now_unix_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub fn open_release_url(url: &str) -> anyhow::Result<()> {
    if !is_allowed_release_url(url) {
        anyhow::bail!("refusing to open non-release URL: {url}");
    }
    crate::desktop::browser::open_url(url)
}

fn is_allowed_release_url(url: &str) -> bool {
    url == RELEASES_PAGE_URL || url.starts_with(RELEASE_URL_PREFIX)
}

#[cfg(test)]
#[path = "releases_test.rs"]
mod updates_test;
