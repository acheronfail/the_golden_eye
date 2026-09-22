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
    let env_config = crate::config::UpdateEnvConfig::from_env();
    env_config.log();
    let releases = tokio::time::timeout(Duration::from_secs(60), fetch_releases(&env_config.releases_api_url()))
        .await
        .context("release history request timed out")??;
    select_update_from_releases(current_version, releases, env_config.include_prereleases())
}

async fn fetch_releases(url: &str) -> anyhow::Result<Vec<GithubRelease>> {
    let client = reqwest::Client::builder().timeout(UPDATE_CHECK_TIMEOUT).build()?;
    let origin = reqwest::Url::parse(url)?;
    let mut next = Some(origin.clone());
    let mut visited = std::collections::HashSet::new();
    let mut releases = Vec::new();
    while let Some(url) = next {
        anyhow::ensure!(url.origin() == origin.origin(), "release pagination changed origin");
        anyhow::ensure!(visited.insert(url.clone()), "release pagination repeated a page");
        let response = client
            .get(url)
            .header(reqwest::header::USER_AGENT, "the-golden-eye-obs-plugin")
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .context("requesting GitHub releases")?;
        if response.status() == StatusCode::NOT_FOUND {
            anyhow::bail!("release history not found");
        }
        let response = response.error_for_status().context("GitHub release API returned an error")?;
        next = next_release_page(response.headers())?;
        releases.extend(releases_from_response(response.json().await.context("parsing GitHub release response")?)?);
    }
    Ok(releases)
}

fn next_release_page(headers: &reqwest::header::HeaderMap) -> anyhow::Result<Option<reqwest::Url>> {
    for link in headers.get_all(reqwest::header::LINK) {
        for part in link.to_str()?.split(',') {
            let mut fields = part.trim().split(';');
            let target = fields.next().unwrap_or_default().trim();
            if fields.any(|field| {
                field
                    .trim()
                    .strip_prefix("rel=")
                    .is_some_and(|rel| rel.trim_matches('"').split_whitespace().any(|value| value == "next"))
            }) {
                let target = target
                    .strip_prefix('<')
                    .and_then(|value| value.strip_suffix('>'))
                    .context("invalid release pagination link")?;
                return Ok(Some(reqwest::Url::parse(target)?));
            }
        }
    }
    Ok(None)
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
    let installed_updater_version = installed_updater_version()?;
    let mut compatible: Option<(Version, GithubRelease, u32)> = None;
    let mut incompatible: Option<(Version, GithubRelease, u32)> = None;
    for release in releases {
        if release.draft || (!include_prereleases && release.prerelease) {
            continue;
        }
        let Ok(version) = parse_version(&release.tag_name) else { continue };
        if version <= current {
            continue;
        }
        let updater = match updater_version_from_assets(
            &version,
            &release.assets,
            std::env::consts::OS,
            std::env::consts::ARCH,
        ) {
            Ok(updater) => updater,
            Err(error) => {
                tracing::warn!(tag = %release.tag_name, %error, "skipping release without a valid platform package");
                continue;
            }
        };
        let best = if updater == installed_updater_version { &mut compatible } else { &mut incompatible };
        if best.as_ref().is_none_or(|(previous, _, _)| version > *previous) {
            *best = Some((version, release, updater));
        }
    }
    let Some((_, release, updater_version)) = compatible.or(incompatible) else { return Ok(None) };
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
