use super::EnvVar;

static GE_UPDATE_CHECK_URL: EnvVar = EnvVar::new("GE_UPDATE_CHECK_URL");

/// GitHub release history used for compatible update selection.
pub(crate) const RELEASES_API_URL: &str =
    "https://api.github.com/repos/acheronfail/the_golden_eye/releases?per_page=100";

/// Controls the update-check endpoint override.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UpdateEnvConfig {
    pub(crate) check_url_override: Option<String>,
}

impl UpdateEnvConfig {
    /// Reads the plugin update endpoint override from `GE_UPDATE_CHECK_URL`.
    pub(crate) fn from_env() -> Self {
        Self::from_values(GE_UPDATE_CHECK_URL.string())
    }

    /// Builds update configuration from raw values for tests and non-environment callers.
    pub(crate) fn from_values(check_url: Option<String>) -> Self {
        Self { check_url_override: check_url }
    }

    /// Returns the GitHub release API URL after applying update endpoint overrides.
    pub(crate) fn releases_api_url(&self) -> String {
        if let Some(url) = &self.check_url_override {
            return url.clone();
        }
        RELEASES_API_URL.to_owned()
    }

    /// Logs active update environment overrides so support logs show non-default update behavior.
    pub(crate) fn log(&self) {
        if let Some(url) = &self.check_url_override {
            tracing::info!(env = GE_UPDATE_CHECK_URL.key(), url = %url, "plugin update check URL overridden by environment");
        }
    }
}
