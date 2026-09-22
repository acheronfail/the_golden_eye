use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, Default)]
pub struct RuntimeConfig {
    pub debug: bool,
    pub timing: bool,
    pub threads_overridden: bool,
}

static RUNTIME_CONFIG: OnceLock<RuntimeConfig> = OnceLock::new();

pub fn configure(config: RuntimeConfig) {
    let _ = RUNTIME_CONFIG.set(config);
}

pub(super) fn runtime_config() -> RuntimeConfig {
    RUNTIME_CONFIG.get().copied().unwrap_or_default()
}

// Set GE_CV_DEBUG to dump intermediate match scores/detections to stderr.
macro_rules! dbg_cv {
    ($($arg:tt)*) => {
        if $crate::config::runtime_config().debug { eprintln!($($arg)*); }
    };
}

static TEMPLATE_DIR: OnceLock<String> = OnceLock::new();

pub fn set_template_dir(path: String) {
    let _ = TEMPLATE_DIR.set(path);
}

pub fn template_dir() -> Option<String> {
    TEMPLATE_DIR.get().cloned()
}

pub(super) use dbg_cv;
