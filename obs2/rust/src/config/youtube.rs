use super::EnvVar;

static GE_YOUTUBE_ENABLED: EnvVar = EnvVar::new("GE_YOUTUBE_ENABLED");
#[cfg(feature = "test-hooks")]
static GE_TEST_YOUTUBE_AUTH_URL: EnvVar = EnvVar::new("GE_TEST_YOUTUBE_AUTH_URL");
#[cfg(feature = "test-hooks")]
static GE_TEST_YOUTUBE_CLIENT_ID: EnvVar = EnvVar::new("GE_TEST_YOUTUBE_CLIENT_ID");
#[cfg(feature = "test-hooks")]
static GE_TEST_YOUTUBE_CLIENT_SECRET: EnvVar = EnvVar::new("GE_TEST_YOUTUBE_CLIENT_SECRET");
#[cfg(feature = "test-hooks")]
static GE_TEST_YOUTUBE_FORCE_KEYRING_FAILURE: EnvVar = EnvVar::new("GE_TEST_YOUTUBE_FORCE_KEYRING_FAILURE");
#[cfg(feature = "test-hooks")]
static GE_TEST_YOUTUBE_OAUTH_STATE: EnvVar = EnvVar::new("GE_TEST_YOUTUBE_OAUTH_STATE");
#[cfg(feature = "test-hooks")]
static GE_TEST_YOUTUBE_REDIRECT_URI: EnvVar = EnvVar::new("GE_TEST_YOUTUBE_REDIRECT_URI");
#[cfg(feature = "test-hooks")]
static GE_TEST_YOUTUBE_TOKEN_FILE: EnvVar = EnvVar::new("GE_TEST_YOUTUBE_TOKEN_FILE");
#[cfg(feature = "test-hooks")]
static GE_TEST_YOUTUBE_TOKEN_URL: EnvVar = EnvVar::new("GE_TEST_YOUTUBE_TOKEN_URL");
#[cfg(feature = "test-hooks")]
static GE_TEST_YOUTUBE_UPLOAD_URL: EnvVar = EnvVar::new("GE_TEST_YOUTUBE_UPLOAD_URL");
#[cfg(feature = "test-hooks")]
static GE_TEST_YOUTUBE_USERINFO_URL: EnvVar = EnvVar::new("GE_TEST_YOUTUBE_USERINFO_URL");

/// Compile-time value of GE_YOUTUBE_ENABLED
const BUILD_TIME_ENABLED: &str = match option_env!("GE_YOUTUBE_ENABLED") {
    Some(value) => value,
    None => "",
};

/// Compile-time YouTube OAuth client ID, injected in CI. Empty in local builds.
pub(crate) const CLIENT_ID: &str = match option_env!("GE_YOUTUBE_CLIENT_ID") {
    Some(value) => value,
    None => "",
};

/// Resolves the build-time YouTube client secret, with test-only runtime
/// overrides kept behind the `test-hooks` feature.
pub(crate) fn client_secret() -> String {
    #[cfg(feature = "test-hooks")]
    if let Some(secret) = GE_TEST_YOUTUBE_CLIENT_SECRET.string() {
        return secret;
    }

    obfstr::obfstring!(match option_env!("GE_YOUTUBE_CLIENT_SECRET") {
        Some(value) => value,
        None => "",
    })
}
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

/// Whether the YouTube UI/API is enabled: requires the runtime flag plus a
/// resolved client ID and secret. Warns when the flag is set but credentials are
/// missing, to explain why the feature stays hidden. Takes the already-resolved
/// endpoints and secret so the caller reads them only once.
pub(crate) fn youtube_enabled(endpoints: &YoutubeEndpoints, client_secret: &str) -> bool {
    if !EnvVar::truthy_value(BUILD_TIME_ENABLED) && !GE_YOUTUBE_ENABLED.truthy() {
        return false;
    }
    let client_id_present = !endpoints.client_id.is_empty();
    let client_secret_present = !client_secret.is_empty();
    if !client_id_present || !client_secret_present {
        tracing::warn!(
            client_id_present,
            client_secret_present,
            "YouTube is enabled but the client ID and/or secret are missing; leaving disabled"
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

    // Test-only overrides use GE_TEST_* names so they are clearly distinct from
    // the real GE_YOUTUBE_* build/runtime configuration.
    #[cfg(feature = "test-hooks")]
    pub(crate) fn resolve() -> Self {
        Self {
            client_id: GE_TEST_YOUTUBE_CLIENT_ID.string().unwrap_or_else(|| CLIENT_ID.to_owned()),
            auth_url: GE_TEST_YOUTUBE_AUTH_URL.string().unwrap_or_else(|| AUTH_URL.to_owned()),
            token_url: GE_TEST_YOUTUBE_TOKEN_URL.string().unwrap_or_else(|| TOKEN_URL.to_owned()),
            upload_url: GE_TEST_YOUTUBE_UPLOAD_URL.string().unwrap_or_else(|| UPLOAD_URL.to_owned()),
            userinfo_url: GE_TEST_YOUTUBE_USERINFO_URL.string().unwrap_or_else(|| USERINFO_URL.to_owned()),
            redirect_uri: GE_TEST_YOUTUBE_REDIRECT_URI.string().unwrap_or_else(|| REDIRECT_URI.to_owned()),
        }
    }
}

/// Test-only override for the OAuth `state` value so the callback can be driven
/// deterministically. Always `None` in shipping builds.
#[cfg(feature = "test-hooks")]
pub(crate) fn test_oauth_state() -> Option<String> {
    GE_TEST_YOUTUBE_OAUTH_STATE.string()
}

/// Test-only override that stores OAuth tokens in a plain file instead of the OS
/// keyring, so integration tests do not require a platform secret store. Always
/// `None` in shipping builds.
#[cfg(feature = "test-hooks")]
pub(crate) fn token_file_override() -> Option<std::path::PathBuf> {
    GE_TEST_YOUTUBE_TOKEN_FILE.string().map(std::path::PathBuf::from)
}

#[cfg(feature = "test-hooks")]
pub(crate) fn force_keyring_failure() -> bool {
    GE_TEST_YOUTUBE_FORCE_KEYRING_FAILURE.truthy()
}
