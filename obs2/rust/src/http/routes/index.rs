use std::sync::LazyLock;

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Result};

use crate::http::AppState;

// The bundled web app, built by cmake.
const APP_BUNDLE_HTML: &str = include_str!(env!("BROWSER_BUNDLE"));

/// A stable content hash of the embedded SPA bundle, used as a build id so a
/// stale browser tab (an older cached page, or one left open across a plugin
/// update) can tell it is out of date and reload. FNV-1a keeps this
/// deterministic across compilers and Rust versions: the value injected into
/// the served HTML and the one the WebSocket reports must agree for pages served
/// by the same build, and differ across builds. Any difference means the tab is
/// running a different frontend than the backend serves, so it reloads.
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

/// The served HTML, with the build id injected as a `<meta>` tag so the running
/// page can read the exact build it was served from (see the SPA's version
/// handshake). Built once and reused. Injected before `</head>` when present, so
/// the tag lives in the document head; otherwise prepended so it is always
/// there.
static APP_BUNDLE_WITH_ID: LazyLock<String> = LazyLock::new(|| {
    let tag = format!("<meta name=\"ge-build-id\" content=\"{}\">", *BUILD_ID);
    match APP_BUNDLE_HTML.find("</head>") {
        Some(idx) => {
            let mut html = String::with_capacity(APP_BUNDLE_HTML.len() + tag.len());
            html.push_str(&APP_BUNDLE_HTML[..idx]);
            html.push_str(&tag);
            html.push_str(&APP_BUNDLE_HTML[idx..]);
            html
        }
        None => format!("{tag}{APP_BUNDLE_HTML}"),
    }
});

#[axum::debug_handler]
pub async fn handler(State(_): State<AppState>) -> Result<impl IntoResponse> {
    Ok((
        [
            (header::CONTENT_TYPE, "text/html"),
            // Never cache the SPA entry point: the build id lives in the HTML, so
            // a cached copy would let a stale tab reload straight back into the
            // old build (a reload loop). Always fetching it fresh means a reload
            // lands on the current build.
            (header::CACHE_CONTROL, "no-store"),
        ],
        APP_BUNDLE_WITH_ID.as_str(),
    ))
}
