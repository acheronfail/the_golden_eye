use super::shared::env_string;

const FLATPAK_ID: &str = "FLATPAK_ID";
const XDG_RUNTIME_DIR: &str = "XDG_RUNTIME_DIR";

/// Controls Flatpak-specific host path hints for frame-dump diagnostics.
pub(crate) fn flatpak_id() -> Option<String> {
    env_string(FLATPAK_ID)
}

/// Controls Flatpak runtime directory mapping for frame-dump diagnostics.
pub(crate) fn xdg_runtime_dir() -> Option<String> {
    env_string(XDG_RUNTIME_DIR)
}
