//! Downloads, verifies, stages, and applies plugin updates while OBS runs (`updates.rs`
//! only detects/announces). Picks the release asset, verifies vs `checksums.txt`, stages
//! into `.ge_update_staged` (a name shared with reload.c/plugin.c), then asks the shim.

use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::Context;
use sha2::{Digest, Sha256};

use crate::http::{AppState, AppStateInner};
use crate::updates::{GithubAsset, PluginUpdate};

const STAGED_DIR_NAME: &str = ".ge_update_staged";
const DOWNLOAD_DIR_NAME: &str = ".ge_update_staged.download";
const CHECKSUMS_ASSET_NAME: &str = "checksums.txt";
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_APPLY_CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// Serializes staging: concurrent callers (e.g. startup auto-check plus a manual
/// "check now") share the single `.ge_update_staged{,.download}` dirs, so without
/// this one run could clobber the other mid-copy and leave nothing staged.
static STAGE_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

/// The exact `<platform>-<arch>` suffix `Package.cmake` bakes into release zip names
/// (`the_golden_eye-<version>-<suffix>.zip`), including its `aarch64` -> `arm64`
/// normalization (Rust reports `"aarch64"`, while release assets use `"arm64"`).
fn platform_arch_suffix_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("macos-arm64"),
        ("macos", "x86_64") => Some("macos-x86_64"),
        ("windows", "x86_64") => Some("windows-x86_64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        _ => None,
    }
}

#[cfg(test)]
fn platform_arch_suffix() -> Option<&'static str> {
    platform_arch_suffix_for(std::env::consts::OS, std::env::consts::ARCH)
}

fn release_version_for_asset(version: &str) -> String {
    version.trim().trim_start_matches('v').to_owned()
}

fn asset_zip_name_for(version: &str, os: &str, arch: &str) -> Option<String> {
    let suffix = platform_arch_suffix_for(os, arch)?;
    Some(format!("the_golden_eye-{}-{suffix}.zip", release_version_for_asset(version)))
}

fn asset_zip_name(update: &PluginUpdate) -> anyhow::Result<String> {
    asset_zip_name_for(&update.latest_version, std::env::consts::OS, std::env::consts::ARCH)
        .context("unsupported OS/arch for auto-update")
}

/// The shim's canonical on-disk path for this core library, set by `ge_core_load` via
/// `ge_rust_set_core_path` (see `lib.rs::core_path`). NOT `ge_obs_module_binary_path()`,
/// which reports the *shim's* file (the OBS-registered module), not the core's.
fn canonical_core_path() -> anyhow::Result<PathBuf> {
    crate::core_path().context("core canonical path not set (ge_core_load hasn't run yet?)")
}

fn install_dir() -> anyhow::Result<PathBuf> {
    canonical_core_path()?.parent().map(Path::to_path_buf).context("core binary path has no parent directory")
}

/// Whether a verified update is currently staged and ready to apply.
pub fn has_staged_update() -> bool {
    let Ok(dir) = install_dir() else { return false };
    let Ok(core_path) = canonical_core_path() else { return false };
    let Some(leaf) = core_path.file_name() else { return false };
    dir.join(STAGED_DIR_NAME).join(leaf).is_file()
}

/// No active monitor session and no in-flight recording/replay-buffer activity.
/// Shared by the auto-apply loop and manual "apply now" -- re-check immediately
/// before triggering (not just when staged) to close the "was safe"/"still safe" gap.
pub fn is_safe_to_apply(state: &AppStateInner) -> bool {
    let no_monitor = state.monitor.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).is_none();
    let no_recording = state.recording_state.current().is_none();
    let no_replay_buffer = !crate::recording::replay_buffer_active();
    no_monitor && no_recording && no_replay_buffer
}

/// Wakes the shim's reload worker to apply whatever is staged. Must run on a plain OS
/// thread, never a tokio worker of the runtime being torn down: `ge_rust_stop()` (the
/// reload triggers it) blocks, and tokio refuses to drop a runtime from its own worker.
pub fn trigger_apply() {
    std::thread::spawn(|| unsafe { crate::ffi::ge_core_trigger_reload() });
}

/// Background task: periodically applies a staged update when opted in
/// (`autoUpdateEnabled`) and safe. Spawned once from `ge_rust_start`. Dev builds
/// always count as opted in and poll faster (hot-reload fallback for `just dev`).
pub async fn auto_apply_when_safe(state: AppState) {
    let poll_interval = if cfg!(feature = "dev") { Duration::from_secs(2) } else { AUTO_APPLY_CHECK_INTERVAL };
    let mut interval = tokio::time::interval(poll_interval);
    loop {
        interval.tick().await;
        if !cfg!(feature = "dev") && !state.settings.get().auto_update_enabled {
            continue;
        }
        if !has_staged_update() {
            continue;
        }
        if !is_safe_to_apply(&state) {
            continue;
        }
        tracing::info!("a staged update is ready and safe to apply");
        trigger_apply();
    }
}

/// Downloads, verifies, and stages the release matching `update`. On success
/// `.ge_update_staged/` holds a checksum-verified core plus best-effort
/// `cv_templates`/`locale`, ready for `trigger_apply`. `assets` is reused from `updates.rs`.
pub async fn download_verify_and_stage(update: &PluginUpdate, assets: Vec<GithubAsset>) -> anyhow::Result<()> {
    let _stage_guard = STAGE_LOCK.lock().await;
    let install_dir = install_dir()?;
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

    // A working directory distinct from STAGED_DIR_NAME, so the shim (which
    // only ever looks for STAGED_DIR_NAME) never sees a
    // half-downloaded/half-extracted update.
    let download_dir = install_dir.join(DOWNLOAD_DIR_NAME);
    let _ = std::fs::remove_dir_all(&download_dir);
    std::fs::create_dir_all(&download_dir).context("creating update download directory")?;

    let zip_path = download_dir.join("release.zip");
    download_to_file(&client, &zip_asset.browser_download_url, &zip_path).await?;

    let actual_sha256 = sha256_of_file(&zip_path)?;
    if !actual_sha256.eq_ignore_ascii_case(&expected_sha256) {
        let _ = std::fs::remove_dir_all(&download_dir);
        anyhow::bail!(
            "downloaded release failed checksum verification (expected {expected_sha256}, got {actual_sha256})"
        );
    }

    let extracted_dir = download_dir.join("extracted");
    extract_zip(&zip_path, &extracted_dir)?;

    let core_src = find_named(&extracted_dir, &core_leaf_name)
        .with_context(|| format!("release package does not contain '{}'", core_leaf_name.to_string_lossy()))?;

    // Only now -- after everything above has succeeded -- touch
    // STAGED_DIR_NAME, and only ever by replacing it wholesale.
    let staged_dir = install_dir.join(STAGED_DIR_NAME);
    let _ = std::fs::remove_dir_all(&staged_dir);
    std::fs::create_dir_all(&staged_dir).context("creating staged update directory")?;
    std::fs::copy(&core_src, staged_dir.join(&core_leaf_name)).context("staging core library")?;

    for data_dir_name in ["cv_templates", "locale"] {
        if let Some(src) = find_named(&extracted_dir, OsStr::new(data_dir_name)) {
            copy_dir_recursive(&src, &staged_dir.join(data_dir_name))
                .with_context(|| format!("staging {data_dir_name}"))?;
        }
    }

    let _ = std::fs::remove_dir_all(&download_dir);
    tracing::info!(version = %update.latest_version, "staged plugin update, ready to apply");
    Ok(())
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
mod tests {
    use super::*;

    #[test]
    fn current_platform_is_supported_when_the_arch_can_be_packaged() {
        assert_eq!(platform_arch_suffix().is_some(), matches!(std::env::consts::ARCH, "aarch64" | "x86_64"));
    }

    #[test]
    fn asset_zip_names_match_the_package_contract() {
        let cases = [
            ("v1.2.3", "macos", "aarch64", "the_golden_eye-1.2.3-macos-arm64.zip"),
            ("1.2.3", "macos", "x86_64", "the_golden_eye-1.2.3-macos-x86_64.zip"),
            ("v1.2.3-beta.1", "linux", "x86_64", "the_golden_eye-1.2.3-beta.1-linux-x86_64.zip"),
            ("v1.2.3+build.4", "linux", "aarch64", "the_golden_eye-1.2.3+build.4-linux-arm64.zip"),
            ("v1.2.3", "windows", "x86_64", "the_golden_eye-1.2.3-windows-x86_64.zip"),
        ];

        for (version, os, arch, expected) in cases {
            assert_eq!(asset_zip_name_for(version, os, arch).as_deref(), Some(expected));
        }
        assert_eq!(asset_zip_name_for("v1.2.3", "windows", "aarch64"), None);
        assert_eq!(asset_zip_name_for("v1.2.3", "freebsd", "x86_64"), None);
    }

    #[test]
    fn find_named_searches_nested_directories() {
        let dir = tempdir();
        std::fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
        std::fs::write(dir.path().join("a/b/c/target.txt"), b"hi").unwrap();

        let found = find_named(dir.path(), OsStr::new("target.txt"));
        assert_eq!(found, Some(dir.path().join("a/b/c/target.txt")));
        assert_eq!(find_named(dir.path(), OsStr::new("missing.txt")), None);
    }

    #[test]
    fn copy_dir_recursive_preserves_structure() {
        let dir = tempdir();
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("nested")).unwrap();
        std::fs::write(src.join("nested/file.txt"), b"hi").unwrap();

        let dst = dir.path().join("dst");
        copy_dir_recursive(&src, &dst).unwrap();
        assert_eq!(std::fs::read(dst.join("nested/file.txt")).unwrap(), b"hi");
    }

    fn tempdir() -> TestDir {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("ge-update-apply-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        TestDir(path)
    }

    struct TestDir(PathBuf);
    impl TestDir {
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
