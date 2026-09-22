use std::process::Command;

/// Opens a URL in the user's default browser via the platform handler.
pub fn open_url(url: &str) -> anyhow::Result<()> {
    tracing::info!(url = url, "opening URL");

    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(url).status();

    #[cfg(target_os = "windows")]
    let status = Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).status();

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let status = Command::new("xdg-open").arg(url).status();

    let status = status?;
    if status.success() { Ok(()) } else { anyhow::bail!("browser opener exited with status {status}") }
}
