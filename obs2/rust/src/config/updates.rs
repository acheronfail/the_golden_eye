use super::shared::{env_string, env_value_enabled};

const GE_UPDATE_CHECK_URL: &str = "GE_UPDATE_CHECK_URL";
const GE_UPDATE_INCLUDE_PRERELEASES: &str = "GE_UPDATE_INCLUDE_PRERELEASES";

/// Default GitHub endpoint used for stable plugin update checks.
pub(crate) const LATEST_RELEASE_API_URL: &str =
    "https://api.github.com/repos/acheronfail/the_golden_eye/releases/latest";
/// GitHub endpoint used when update checks are configured to include pre-releases.
pub(crate) const RELEASES_API_URL: &str = "https://api.github.com/repos/acheronfail/the_golden_eye/releases";

/// Controls update-check endpoint overrides and whether pre-releases are considered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UpdateEnvConfig {
    pub(crate) check_url_override: Option<String>,
    pub(crate) include_prereleases_override: Option<bool>,
}

impl UpdateEnvConfig {
    /// Reads plugin update overrides from `GE_UPDATE_CHECK_URL` and `GE_UPDATE_INCLUDE_PRERELEASES`.
    pub(crate) fn from_env() -> Self {
        Self::from_values(env_string(GE_UPDATE_CHECK_URL), env_string(GE_UPDATE_INCLUDE_PRERELEASES))
    }

    /// Builds update configuration from raw values for tests and non-environment callers.
    pub(crate) fn from_values(check_url: Option<String>, include_prereleases: Option<String>) -> Self {
        Self {
            check_url_override: check_url,
            include_prereleases_override: include_prereleases.map(|value| env_value_enabled(&value)),
        }
    }

    /// Reports whether update selection should include GitHub pre-releases.
    pub(crate) fn include_prereleases(&self) -> bool {
        self.include_prereleases_override.unwrap_or(false)
    }

    /// Returns the GitHub release API URL after applying update endpoint overrides.
    pub(crate) fn releases_api_url(&self) -> String {
        if let Some(url) = &self.check_url_override {
            return url.clone();
        }
        if self.include_prereleases() { RELEASES_API_URL.to_owned() } else { LATEST_RELEASE_API_URL.to_owned() }
    }

    /// Logs active update environment overrides so support logs show non-default update behavior.
    pub(crate) fn log(&self) {
        if let Some(url) = &self.check_url_override {
            tracing::info!(env = GE_UPDATE_CHECK_URL, url = %url, "plugin update check URL overridden by environment");
        }
        if let Some(include_prereleases) = self.include_prereleases_override {
            tracing::info!(
                env = GE_UPDATE_INCLUDE_PRERELEASES,
                include_prereleases,
                "plugin update pre-release selection overridden by environment"
            );
        }
        if self.check_url_override.is_some() && self.include_prereleases_override.is_some() {
            tracing::warn!(
                url_env = GE_UPDATE_CHECK_URL,
                ignored_for_endpoint_env = GE_UPDATE_INCLUDE_PRERELEASES,
                "plugin update URL override takes precedence; pre-release env var will not change the release API endpoint"
            );
        }
    }
}
