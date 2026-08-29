use std::sync::LazyLock;

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Result};

use crate::config;
use crate::http::AppState;

// The bundled web app, built by cmake.
const APP_BUNDLE_HTML: &str = include_str!(env!("BROWSER_BUNDLE"));

/// Stable content hash (FNV-1a, deterministic across compilers) of the embedded
/// SPA bundle, used as a build id so a stale tab can detect it differs from the
/// backend's build and reload. Injected into the HTML and reported over WebSocket.
pub static BUILD_ID: LazyLock<String> = LazyLock::new(|| {
    // FNV-1a (64-bit) over the raw bundle bytes -- the id is injected into the
    // served copy below, so hashing the raw const (before injection) is what
    // keeps it self-consistent rather than circular.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in APP_BUNDLE_HTML.as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
});

fn inject_meta_tags(html: &str, tags: &str) -> String {
    match html.find("</head>") {
        Some(idx) => {
            let mut injected = String::with_capacity(html.len() + tags.len());
            injected.push_str(&html[..idx]);
            injected.push_str(tags);
            injected.push_str(&html[idx..]);
            injected
        }
        None => format!("{tags}{html}"),
    }
}

fn app_bundle_html() -> String {
    let mut tags = format!("<meta name=\"ge-build-id\" content=\"{}\">", *BUILD_ID);
    if config::browser_ws_log_enabled() {
        tags.push_str("<meta name=\"ge-browser-ws-log\" content=\"1\">");
    }
    inject_meta_tags(APP_BUNDLE_HTML, &tags)
}

#[axum::debug_handler]
pub async fn handler(State(_): State<AppState>) -> Result<impl IntoResponse> {
    Ok((
        [
            (header::CONTENT_TYPE, "text/html"),
            // Never cache the SPA entry point: the build id lives in the HTML, so
            // a cached copy would reload a stale tab straight back into the old
            // build (a reload loop). Fresh fetches land on the current build.
            (header::CACHE_CONTROL, "no-store"),
        ],
        app_bundle_html(),
    ))
}
