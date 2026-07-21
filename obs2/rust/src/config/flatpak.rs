use super::EnvVar;

static FLATPAK_ID: EnvVar = EnvVar::new("FLATPAK_ID");
static XDG_RUNTIME_DIR: EnvVar = EnvVar::new("XDG_RUNTIME_DIR");

/// Controls Flatpak-specific host path hints for frame-dump diagnostics.
pub(crate) fn flatpak_id() -> Option<String> {
    FLATPAK_ID.string()
}

/// Controls Flatpak runtime directory mapping for frame-dump diagnostics.
pub(crate) fn xdg_runtime_dir() -> Option<String> {
    XDG_RUNTIME_DIR.string()
}
