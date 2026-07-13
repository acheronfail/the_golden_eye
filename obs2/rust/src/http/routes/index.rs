use std::sync::LazyLock;

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Result};

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

/// The served HTML with the build id injected as a `<meta>` tag so the page can
/// read the build it was served from (see the SPA version handshake). Built once;
/// injected before `</head>` when present, otherwise prepended.
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
            // a cached copy would reload a stale tab straight back into the old
            // build (a reload loop). Fresh fetches land on the current build.
            (header::CACHE_CONTROL, "no-store"),
        ],
        APP_BUNDLE_WITH_ID.as_str(),
    ))
}
