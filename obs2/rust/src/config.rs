//! Application configuration, resolved from the environment once at startup
//! (just after logging is initialised) and stored in the shared app state.

/// Application configuration, resolved from the environment. Each field is
/// `None` when its variable is absent or empty.
pub struct Config {
    pub discord_message_name: Option<String>,
    pub discord_webhook_url: Option<String>,
}

impl Config {
    /// Loads all configuration from the environment, logging which variables
    /// were found and which were missing.
    pub fn from_env() -> Self {
        Self {
            discord_message_name: read_env("DISCORD_MESSAGE_NAME"),
            discord_webhook_url: read_env("DISCORD_WEBHOOK_URL"),
        }
    }
}

/// Reads an environment variable, logging whether it was found.
fn read_env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => {
            tracing::info!("config: {name} is set");
            Some(value)
        }
        _ => {
            tracing::warn!("config: {name} is not set");
            None
        }
    }
}
