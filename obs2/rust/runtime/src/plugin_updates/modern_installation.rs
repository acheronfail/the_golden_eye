//! One-time provisioning for OBS's modern discovery path; existing installs are never synchronized.
use std::path::Path;

use anyhow::Context;

use super::installation::UpdateWorkDir;
#[cfg(target_os = "linux")]
use super::installation::copy_dir_recursive;

pub(crate) fn provision() -> anyhow::Result<()> {
    if cfg!(feature = "dev") || crate::config::core_path_overridden() {
        return Ok(());
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    if let (Some(core), Some(data)) = (crate::core_path(), crate::obs::module_data_path()) {
        #[cfg(target_os = "linux")]
        if let Some((legacy, modern)) = crate::config::obs_plugin_roots() {
            let source = legacy.join("the_golden_eye");
            if core == source.join("bin/64bit/libgolden_core.so") && data == source.join("data") {
                provision_linux(&source, &modern.join("the_golden_eye"))?;
            }
        }
        #[cfg(target_os = "windows")]
        if let Some(root) = core.parent().and_then(Path::parent).and_then(Path::parent)
            && core == root.join("bin/64bit/golden_core.dll")
            && data == root.join("data")
        {
            provision_windows(root)?;
        }
    }
    Ok(())
}

fn reject_symlinks(path: &Path) -> anyhow::Result<()> {
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.file_type().is_symlink() => anyhow::bail!("symbolic link at '{}'", ancestor.display()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| format!("inspecting '{}'", ancestor.display())),
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn provision_linux(source: &Path, destination: &Path) -> anyhow::Result<()> {
    reject_symlinks(source)?;
    reject_symlinks(destination)?;
    if destination.try_exists()? {
        reject_symlinks(&destination.join("the_golden_eye.so"))?;
        anyhow::ensure!(
            destination.join("the_golden_eye.so").is_file(),
            "'{}' already exists without a modern plugin loader; install the current release there manually",
            destination.display()
        );
        return Ok(());
    }
    let core = source.join("bin/64bit/libgolden_core.so");
    reject_symlinks(&core)?;
    let loader = source.join("data/modern-loader/the_golden_eye.so");
    reject_symlinks(&loader)?;
    let parent = destination.parent().context("modern plugin directory has no parent")?;
    std::fs::create_dir_all(parent)?;
    let work = UpdateWorkDir::create(destination)?;
    let incoming = work.path().join("plugin");
    std::fs::create_dir_all(incoming.join("bin/64bit"))?;
    std::fs::copy(core, incoming.join("bin/64bit/libgolden_core.so"))?;
    std::fs::copy(loader, incoming.join("the_golden_eye.so"))?;
    copy_dir_recursive(&source.join("data"), &incoming.join("data"))?;
    // RENAME_NOREPLACE also protects a directory created by another installer during the copy.
    use std::os::unix::ffi::OsStrExt;
    let from = std::ffi::CString::new(incoming.as_os_str().as_bytes())?;
    let to = std::ffi::CString::new(destination.as_os_str().as_bytes())?;
    // SAFETY: both paths are valid NUL-terminated strings, and neither descriptor is used.
    if unsafe { libc::renameat2(libc::AT_FDCWD, from.as_ptr(), libc::AT_FDCWD, to.as_ptr(), libc::RENAME_NOREPLACE) }
        != 0
    {
        return Err(std::io::Error::last_os_error()).context("publishing modern plugin installation");
    }
    tracing::info!(path = %destination.display(), "provisioned modern OBS plugin installation");
    Ok(())
}

#[cfg(target_os = "windows")]
fn provision_windows(root: &Path) -> anyhow::Result<()> {
    reject_symlinks(root)?;
    let destination = root.join("the_golden_eye.dll");
    if std::fs::symlink_metadata(&destination).is_ok() {
        reject_symlinks(&destination)?;
        anyhow::ensure!(destination.is_file(), "modern loader path is not a file");
        return Ok(());
    }
    let loader = root.join("data/modern-loader/the_golden_eye.dll");
    reject_symlinks(&loader)?;
    let work = UpdateWorkDir::create(&destination)?;
    let incoming = work.path().join("loader.dll");
    std::fs::copy(loader, &incoming)?;
    // Creating a hard link publishes the complete file atomically and never replaces an existing loader.
    std::fs::hard_link(incoming, &destination).context("publishing modern OBS plugin loader")?;
    tracing::info!(path = %destination.display(), "provisioned modern OBS plugin loader");
    Ok(())
}

#[cfg(test)]
#[path = "modern_installation_test.rs"]
mod tests;
