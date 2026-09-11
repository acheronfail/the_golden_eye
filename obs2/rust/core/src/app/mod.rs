//! Application composition and the state published to connected clients.
mod events;
mod publication;
mod startup;
mod state;
pub use events::AppEvent;
pub use publication::{AppSnapshot, SharedStateStore};
pub(crate) use startup::build_state;
pub use state::{AppState, AppStateInner};

mod settings;
pub(crate) use settings::watch_settings_file;
