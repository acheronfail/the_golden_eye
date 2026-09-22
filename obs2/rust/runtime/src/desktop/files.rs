use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub(crate) enum RevealMode {
    Select,
    Open,
}

pub(crate) fn reveal_in_file_browser(path: PathBuf, mode: RevealMode) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    let status = match mode {
        RevealMode::Select => Command::new("open").arg("-R").arg(&path).status(),
        RevealMode::Open => Command::new("open").arg(&path).status(),
    };

    #[cfg(target_os = "windows")]
    let status = match mode {
        RevealMode::Select => Command::new("explorer").arg(format!("/select,{}", path.display())).status(),
        RevealMode::Open => Command::new("explorer").arg(&path).status(),
    };

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let status = match mode {
        RevealMode::Select => {
            let target = path.parent().unwrap_or_else(|| std::path::Path::new("."));
            Command::new("xdg-open").arg(target).status()
        }
        RevealMode::Open => Command::new("xdg-open").arg(&path).status(),
    };

    let status = status?;
    if status.success() { Ok(()) } else { anyhow::bail!("file browser exited with status {status}") }
}
