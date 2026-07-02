use std::time::Duration;

const LAUNCH_WAIT_TIMEOUT: Duration = Duration::from_secs(10);
const LAUNCH_WAIT_INTERVAL: Duration = Duration::from_millis(200);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);

#[cfg(feature = "dev")]
fn app_url() -> &'static str {
    option_env!("GE_BROWSER_DEV_URL").unwrap_or("http://localhost:5173")
}

#[cfg(not(feature = "dev"))]
fn app_url() -> &'static str {
    "http://localhost:31337"
}

pub async fn open_when_ready() {
    let url = app_url().to_owned();
    wait_for_listener(&url).await;

    let launch_url = url.clone();
    match tokio::task::spawn_blocking(move || webbrowser::open(&launch_url)).await {
        Ok(Ok(())) => tracing::info!(%url, "opened The Golden Eye in the default browser"),
        Ok(Err(err)) => tracing::warn!(%url, "failed to open The Golden Eye in the default browser: {err}"),
        Err(err) => tracing::warn!(%url, "browser launch task failed: {err}"),
    }
}

async fn wait_for_listener(url: &str) {
    let Some(addr) = listener_addr(url) else {
        tracing::warn!(%url, "could not parse app URL before browser launch");
        return;
    };

    let deadline = tokio::time::Instant::now() + LAUNCH_WAIT_TIMEOUT;
    loop {
        match tokio::time::timeout(CONNECT_TIMEOUT, tokio::net::TcpStream::connect(&addr)).await {
            Ok(Ok(_)) => return,
            Ok(Err(_)) | Err(_) => {}
        }

        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(%url, "opening browser before app URL accepted a connection");
            return;
        }

        tokio::time::sleep(LAUNCH_WAIT_INTERVAL).await;
    }
}

fn listener_addr(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let port = parsed.port_or_known_default()?;
    Some(format!("{host}:{port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listener_addr_uses_known_default_port() {
        assert_eq!(listener_addr("http://localhost/").as_deref(), Some("localhost:80"));
        assert_eq!(listener_addr("https://example.com/path").as_deref(), Some("example.com:443"));
    }

    #[test]
    fn listener_addr_uses_explicit_port() {
        assert_eq!(listener_addr("http://localhost:31337/options").as_deref(), Some("localhost:31337"));
    }
}
