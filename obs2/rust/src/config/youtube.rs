use super::shared::env_truthy;

const GE_YOUTUBE_ENABLED: &str = "GE_YOUTUBE_ENABLED";

/// Compile-time YouTube OAuth client ID, injected in CI. Empty in local builds.
pub(crate) const CLIENT_ID: &str = match option_env!("GE_YOUTUBE_CLIENT_ID") {
    Some(value) => value,
    None => "",
};
/// Google OAuth 2.0 authorization endpoint.
pub(crate) const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
/// Google OAuth 2.0 token endpoint.
pub(crate) const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// YouTube Data API resumable upload endpoint.
pub(crate) const UPLOAD_URL: &str = "https://www.googleapis.com/upload/youtube/v3/videos";
/// Google OpenID Connect userinfo endpoint.
pub(crate) const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";
/// Loopback redirect the plugin's local server handles after consent.
pub(crate) const REDIRECT_URI: &str = "http://127.0.0.1:31337/oauth/callback";

/// Whether a client secret was compiled in (injected by CI). Empty in local builds.
const CLIENT_SECRET_PRESENT: bool = match option_env!("GE_YOUTUBE_CLIENT_SECRET") {
    Some(value) => !value.is_empty(),
    None => false,
};

/// Whether the YouTube UI/API is enabled: requires the runtime flag plus a
/// compiled-in client ID and secret. Warns when the flag is set but credentials
/// are missing, to explain why the feature stays hidden. Takes already-resolved
/// endpoints so the caller reads them only once.
pub(crate) fn youtube_enabled(endpoints: &YoutubeEndpoints) -> bool {
    if !env_truthy(GE_YOUTUBE_ENABLED) {
        return false;
    }
    let client_id_present = !endpoints.client_id.is_empty();
    if !client_id_present || !CLIENT_SECRET_PRESENT {
        tracing::warn!(
            client_id_present,
            client_secret_present = CLIENT_SECRET_PRESENT,
            "GE_YOUTUBE_ENABLED is set but the YouTube client ID and/or secret are missing; leaving disabled"
        );
        return false;
    }
    true
}

/// Resolved endpoint/client values. Always the compile-time constants in shipping
/// builds; only the `test-hooks` feature lets env vars redirect them at a mock
/// Google/YouTube surface for integration tests.
#[derive(Clone, Debug)]
pub(crate) struct YoutubeEndpoints {
    pub(crate) client_id: String,
    pub(crate) auth_url: String,
    pub(crate) token_url: String,
    pub(crate) upload_url: String,
    pub(crate) userinfo_url: String,
    pub(crate) redirect_uri: String,
}

impl YoutubeEndpoints {
    #[cfg(not(feature = "test-hooks"))]
    pub(crate) fn resolve() -> Self {
        Self {
            client_id: CLIENT_ID.to_owned(),
            auth_url: AUTH_URL.to_owned(),
            token_url: TOKEN_URL.to_owned(),
            upload_url: UPLOAD_URL.to_owned(),
            userinfo_url: USERINFO_URL.to_owned(),
            redirect_uri: REDIRECT_URI.to_owned(),
        }
    }

    #[cfg(feature = "test-hooks")]
    pub(crate) fn resolve() -> Self {
        use super::shared::env_string;
        Self {
            client_id: env_string("GE_YOUTUBE_CLIENT_ID").unwrap_or_else(|| CLIENT_ID.to_owned()),
            auth_url: env_string("GE_YOUTUBE_AUTH_URL").unwrap_or_else(|| AUTH_URL.to_owned()),
            token_url: env_string("GE_YOUTUBE_TOKEN_URL").unwrap_or_else(|| TOKEN_URL.to_owned()),
            upload_url: env_string("GE_YOUTUBE_UPLOAD_URL").unwrap_or_else(|| UPLOAD_URL.to_owned()),
            userinfo_url: env_string("GE_YOUTUBE_USERINFO_URL").unwrap_or_else(|| USERINFO_URL.to_owned()),
            redirect_uri: env_string("GE_YOUTUBE_REDIRECT_URI").unwrap_or_else(|| REDIRECT_URI.to_owned()),
        }
    }
}

/// Test-only override for the OAuth `state` value so the callback can be driven
/// deterministically. Always `None` in shipping builds.
#[cfg(feature = "test-hooks")]
pub(crate) fn test_oauth_state() -> Option<String> {
    super::shared::env_string("GE_YOUTUBE_TEST_OAUTH_STATE")
}

/// Test-only override that stores OAuth tokens in a plain file instead of the OS
/// keyring, so integration tests do not require a platform secret store. Always
/// `None` in shipping builds.
#[cfg(feature = "test-hooks")]
pub(crate) fn token_file_override() -> Option<std::path::PathBuf> {
    super::shared::env_string("GE_YOUTUBE_TOKEN_FILE").map(std::path::PathBuf::from)
}
