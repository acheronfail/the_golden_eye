//! Plugin update ownership: lifecycle decisions, release discovery, and installation.
pub(crate) mod installation;
mod lifecycle;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod modern_installation;
mod releases;

pub(crate) use lifecycle::{ApplyError, PluginUpdates};
pub use releases::{DownloadUpdateResult, PluginUpdate, UpdatePhase, UpdateStatus};
pub(crate) use releases::{GithubAsset, open_release_url, platform_arch_suffix_for};
