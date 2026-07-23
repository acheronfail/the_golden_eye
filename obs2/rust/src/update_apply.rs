//! Downloads, verifies, stages, and applies plugin updates while OBS runs (`updates.rs`
//! only detects/announces). Picks the release asset, verifies vs `checksums.txt`, stages
//! into the directory supplied by the shim, then asks the shim to reload the core.

use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::Context;
use sha2::{Digest, Sha256};

use crate::http::{AppState, AppStateInner};
use crate::updates::{GithubAsset, PluginUpdate, platform_arch_suffix_for};

const CHECKSUMS_ASSET_NAME: &str = "checksums.txt";
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_APPLY_CHECK_INTERVAL: Duration = Duration::from_secs(30);
const RUNTIME_DATA_DIRS: [&str; 2] = ["cv_templates", "locale"];

/// Serializes publication to the single shim-visible staging directory.
static STAGE_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));
static DATA_SWAP_COUNTER: AtomicU64 = AtomicU64::new(0);
static WORK_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The exact `<platform>-<arch>` suffix `Package.cmake` bakes into release zip names,
/// including its `aarch64` -> `arm64`
/// normalization (Rust reports `"aarch64"`, while release assets use `"arm64"`).
#[cfg(test)]
fn platform_arch_suffix() -> Option<&'static str> {
    platform_arch_suffix_for(std::env::consts::OS, std::env::consts::ARCH)
}

fn release_version_for_asset(version: &str) -> String {
    version.trim().trim_start_matches('v').to_owned()
}

fn asset_zip_name_for(version: &str, updater_version: u32, os: &str, arch: &str) -> Option<String> {
    let suffix = platform_arch_suffix_for(os, arch)?;
    Some(format!("the_golden_eye-u{updater_version}-v{}-{suffix}.zip", release_version_for_asset(version)))
}

fn asset_zip_name(update: &PluginUpdate) -> anyhow::Result<String> {
    asset_zip_name_for(&update.latest_version, update.updater_version, std::env::consts::OS, std::env::consts::ARCH)
        .context("unsupported OS/arch for auto-update")
}

/// The shim's canonical on-disk path for this core library, set by ge_core_load.
fn canonical_core_path() -> anyhow::Result<PathBuf> {
    crate::core_path().context("core canonical path not set")
}

fn staged_dir() -> anyhow::Result<PathBuf> {
    crate::staged_update_dir().context("staged update path not set")
}

fn packaged_core_name() -> &'static OsStr {
    #[cfg(target_os = "windows")]
    return OsStr::new("golden_core.dll");
    #[cfg(target_os = "macos")]
    return OsStr::new("libgolden_core.dylib");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    return OsStr::new("libgolden_core.so");
}

/// Whether a verified update is currently staged and ready to apply.
pub fn has_staged_update() -> bool {
    let Ok(dir) = staged_dir() else { return false };
    let Ok(core_path) = canonical_core_path() else { return false };
    let Some(leaf) = core_path.file_name() else { return false };
    dir.join(leaf).is_file()
}

/// In production, no active monitor session and no in-flight recording activity.
/// A running replay buffer is owned by OBS and survives a core reload, so it
/// does not make applying an update unsafe by itself.
/// Dev builds intentionally relax the monitor/recording checks for hot reload.
/// Shared by the auto-apply loop and manual "apply now" -- re-check immediately
/// before triggering (not just when staged) to close the "was safe"/"still safe" gap.
pub fn is_safe_to_apply(state: &AppStateInner) -> bool {
    let monitor_active = state.monitor.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).is_some();
    let recording_active = state.recording_state.current().is_some();
    activity_is_safe_to_apply(monitor_active, recording_active)
}

fn activity_is_safe_to_apply(monitor_active: bool, recording_active: bool) -> bool {
    cfg!(feature = "dev") || (!monitor_active && !recording_active)
}

/// Wakes the shim's reload worker to apply whatever is staged. Must run on a plain OS
/// thread, never a tokio worker of the runtime being torn down: `ge_rust_stop()` (the
/// reload triggers it) blocks, and tokio refuses to drop a runtime from its own worker.
pub fn trigger_apply() {
    std::thread::spawn(|| unsafe { crate::ffi::ge_core_trigger_reload() });
}

/// Applies a staged update immediately when the frontend is ready and runtime
/// activity is safe. Returns whether a reload was requested.
pub fn trigger_apply_if_safe(state: &AppStateInner) -> bool {
    if !*state.frontend_ready_tx.borrow() || !has_staged_update() || !is_safe_to_apply(state) {
        return false;
    }
    let status = state.snapshot.current_update_status();
    state.snapshot.set_update_status(crate::updates::UpdateStatus {
        phase: crate::updates::UpdatePhase::Applying,
        available: status.available,
    });
    trigger_apply();
    true
}

/// Background task: periodically applies a staged update when opted in
/// (`autoUpdateEnabled`) and safe. Spawned once from `ge_rust_start`. Dev builds
/// always count as opted in and poll faster (hot-reload fallback for `just dev`).
pub async fn auto_apply_when_safe(state: AppState) {
    let poll_interval = if cfg!(feature = "dev") { Duration::from_secs(2) } else { AUTO_APPLY_CHECK_INTERVAL };
    let mut frontend_ready = state.frontend_ready_tx.subscribe();
    while !*frontend_ready.borrow_and_update() {
        if frontend_ready.changed().await.is_err() {
            return;
        }
    }
    let mut interval = tokio::time::interval(poll_interval);
    loop {
        interval.tick().await;
        if !cfg!(feature = "dev") && !state.settings.get().auto_update_enabled {
            continue;
        }
        if trigger_apply_if_safe(&state) {
            tracing::info!("a staged update is ready and safe to apply");
        }
    }
}

/// Downloads, verifies, and stages the release matching `update`. On success
/// `.ge_update_staged/` holds a checksum-verified core and runtime data, ready
/// for `trigger_apply`. `assets` is reused from `updates.rs`.
pub async fn download_verify_and_stage(update: &PluginUpdate, assets: Vec<GithubAsset>) -> anyhow::Result<()> {
    if update.requires_manual_install {
        anyhow::bail!(
            "update {} requires updater u{}, but this installation supports u{}",
            update.latest_version,
            update.updater_version,
            crate::UPDATER_VERSION
        );
    }
    let _stage_guard = STAGE_LOCK.lock().await;
    if has_staged_update() {
        return Ok(());
    }
    let core_leaf_name =
        canonical_core_path()?.file_name().context("core binary path has no file name")?.to_os_string();

    let client = reqwest::Client::builder().timeout(DOWNLOAD_TIMEOUT).build()?;

    let zip_name = asset_zip_name(update)?;
    let zip_asset = assets
        .iter()
        .find(|asset| asset.name == zip_name)
        .with_context(|| format!("release has no asset named '{zip_name}'"))?;
    let checksums_asset =
        assets.iter().find(|asset| asset.name == CHECKSUMS_ASSET_NAME).context("release has no checksums.txt asset")?;

    let expected_sha256 = fetch_expected_sha256(&client, &checksums_asset.browser_download_url, &zip_name).await?;

    // A fresh sibling workspace guarantees extraction cannot reuse files from
    // an earlier attempt and keeps the final publication rename same-volume.
    let staged_dir = staged_dir()?;
    let work_dir = UpdateWorkDir::create(&staged_dir)?;

    let zip_path = work_dir.path().join("release.zip");
    download_to_file(&client, &zip_asset.browser_download_url, &zip_path).await?;

    let actual_sha256 = sha256_of_file(&zip_path)?;
    if !actual_sha256.eq_ignore_ascii_case(&expected_sha256) {
        anyhow::bail!(
            "downloaded release failed checksum verification (expected {expected_sha256}, got {actual_sha256})"
        );
    }

    let extracted_dir = work_dir.path().join("extracted");
    extract_zip(&zip_path, &extracted_dir)?;

    let prepared_dir = work_dir.path().join("prepared");
    prepare_staged_update(&extracted_dir, &prepared_dir, &core_leaf_name)?;

    // Only now touch the directory visible to the shim, replacing it wholesale.
    remove_staged_dir(&staged_dir)?;
    std::fs::rename(&prepared_dir, &staged_dir).context("publishing staged update directory")?;

    tracing::info!(version = %update.latest_version, "staged plugin update, ready to apply");
    Ok(())
}

fn prepare_staged_update(extracted_dir: &Path, prepared_dir: &Path, installed_core_leaf: &OsStr) -> anyhow::Result<()> {
    let core_src = find_named(extracted_dir, packaged_core_name())
        .with_context(|| format!("release package does not contain '{}'", packaged_core_name().to_string_lossy()))?;
    std::fs::create_dir(prepared_dir).context("creating fresh prepared update directory")?;
    std::fs::copy(&core_src, prepared_dir.join(installed_core_leaf)).context("staging core library")?;

    for name in RUNTIME_DATA_DIRS {
        let src = find_named(extracted_dir, OsStr::new(name))
            .with_context(|| format!("release package does not contain runtime data '{name}'"))?;
        copy_dir_recursive(&src, &prepared_dir.join(name)).with_context(|| format!("staging {name}"))?;
    }
    Ok(())
}

struct UpdateWorkDir(PathBuf);

impl UpdateWorkDir {
    fn create(staged_dir: &Path) -> anyhow::Result<Self> {
        let parent = staged_dir.parent().context("staged update path has no parent directory")?;
        for _ in 0..100 {
            let sequence = WORK_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            let candidate = parent.join(format!(".ge-update-work-{}-{sequence}", std::process::id()));
            match std::fs::create_dir(&candidate) {
                Ok(()) => return Ok(Self(candidate)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error).context("creating update workspace"),
            }
        }
        anyhow::bail!("could not allocate a unique update workspace")
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for UpdateWorkDir {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(path = %self.0.display(), "failed to remove update workspace: {error}");
        }
    }
}

fn remove_staged_dir(path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("removing previous staged update"),
    }
}

struct DirectorySwap {
    destination: PathBuf,
    backup: Option<PathBuf>,
}

pub struct RuntimeDataTransaction {
    swaps: Vec<DirectorySwap>,
    committed: bool,
}

impl RuntimeDataTransaction {
    fn install(staged_dir: &Path, data_dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(data_dir).context("creating OBS module data directory")?;
        let mut transaction = Self { swaps: Vec::new(), committed: false };
        for name in RUNTIME_DATA_DIRS {
            let source = staged_dir.join(name);
            if source.is_dir() {
                transaction.install_dir(&source, data_dir, name)?;
            }
        }
        Ok(transaction)
    }

    fn install_dir(&mut self, source: &Path, data_dir: &Path, name: &str) -> anyhow::Result<()> {
        let destination = data_dir.join(name);
        let unique = format!("{}.{}", std::process::id(), DATA_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed));
        let incoming = data_dir.join(format!(".ge-update-{name}-incoming-{unique}"));
        let backup = data_dir.join(format!(".ge-update-{name}-backup-{unique}"));

        if let Err(error) = copy_dir_recursive(source, &incoming) {
            let _ = std::fs::remove_dir_all(&incoming);
            return Err(error).with_context(|| format!("copying staged {name}"));
        }
        let had_old = destination.exists();
        if had_old && let Err(error) = std::fs::rename(&destination, &backup) {
            let _ = std::fs::remove_dir_all(&incoming);
            return Err(error).with_context(|| format!("backing up installed {name}"));
        }
        if let Err(error) = std::fs::rename(&incoming, &destination) {
            if had_old {
                let _ = std::fs::rename(&backup, &destination);
            }
            let _ = std::fs::remove_dir_all(&incoming);
            return Err(error).with_context(|| format!("installing staged {name}"));
        }
        self.swaps.push(DirectorySwap { destination, backup: had_old.then_some(backup) });
        Ok(())
    }

    pub fn commit(mut self) {
        self.committed = true;
        for swap in &self.swaps {
            if let Some(backup) = &swap.backup
                && let Err(error) = std::fs::remove_dir_all(backup)
            {
                tracing::warn!(path = %backup.display(), "failed to remove runtime data backup: {error}");
            }
        }
    }

    fn rollback(&mut self) {
        for swap in self.swaps.iter().rev() {
            if let Err(error) = std::fs::remove_dir_all(&swap.destination) {
                tracing::error!(path = %swap.destination.display(), "failed to remove updated runtime data during rollback: {error}");
            }
            if let Some(backup) = &swap.backup
                && let Err(error) = std::fs::rename(backup, &swap.destination)
            {
                tracing::error!(
                    backup = %backup.display(),
                    destination = %swap.destination.display(),
                    "failed to restore runtime data backup: {error}"
                );
            }
        }
    }
}

impl Drop for RuntimeDataTransaction {
    fn drop(&mut self) {
        if !self.committed {
            self.rollback();
        }
    }
}

pub fn install_staged_runtime_data() -> anyhow::Result<RuntimeDataTransaction> {
    let staged_dir = staged_dir()?;
    let data_dir = crate::read_obs_path(crate::ffi::ge_obs_module_data_path)
        .context("OBS module data path is unavailable during update")?;
    RuntimeDataTransaction::install(&staged_dir, &data_dir)
}

async fn fetch_expected_sha256(client: &reqwest::Client, url: &str, zip_name: &str) -> anyhow::Result<String> {
    let text = client
        .get(url)
        .send()
        .await
        .context("downloading checksums.txt")?
        .error_for_status()
        .context("checksums.txt request returned an error")?
        .text()
        .await
        .context("reading checksums.txt body")?;

    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else { continue };
        let Some(name) = parts.next() else { continue };
        // `sha256sum` output may record a path (e.g. "dist/foo.zip"); compare
        // by basename so exactly how CI invoked it doesn't matter here.
        let basename = Path::new(name).file_name().map(|n| n.to_string_lossy().into_owned());
        if basename.as_deref() == Some(zip_name) {
            return Ok(hash.to_owned());
        }
    }

    anyhow::bail!("checksums.txt has no entry for '{zip_name}'");
}

async fn download_to_file(client: &reqwest::Client, url: &str, dest: &Path) -> anyhow::Result<()> {
    let mut response = client
        .get(url)
        .send()
        .await
        .context("downloading release asset")?
        .error_for_status()
        .context("release asset request returned an error")?;

    let mut file = std::fs::File::create(dest).context("creating download file")?;
    while let Some(chunk) = response.chunk().await.context("reading download stream")? {
        std::io::Write::write_all(&mut file, &chunk).context("writing download file")?;
    }
    Ok(())
}

fn sha256_of_file(path: &Path) -> anyhow::Result<String> {
    let mut file = std::fs::File::open(path).context("opening downloaded file for checksum verification")?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn extract_zip(zip_path: &Path, dest: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::open(zip_path).context("opening downloaded zip")?;
    let mut archive = zip::ZipArchive::new(file).context("reading downloaded zip")?;
    archive.extract(dest).context("extracting downloaded zip")?;
    Ok(())
}

/// Recursively searches `root` for a file or directory whose leaf name is `name`.
/// The release package's layout differs per platform (macOS bundle vs Linux/Windows
/// bin+data), so this searches rather than assuming a fixed relative path.
fn find_named(root: &Path, name: &OsStr) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name() == Some(name) {
            return Some(path);
        }
        if path.is_dir() {
            subdirs.push(path);
        }
    }
    subdirs.into_iter().find_map(|subdir| find_named(&subdir, name))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "update_apply_test.rs"]
mod update_apply_test;
