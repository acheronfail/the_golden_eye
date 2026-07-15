use std::time::{Duration, UNIX_EPOCH};

use super::*;

#[test]
fn notification_template_renders_supported_tokens() {
    let rendered = render_notification_template(
        "url={broadcast_url} utc={timestamp} local={timestamp_local} unix={unix_seconds}",
        "https://youtu.be/abc123",
        UNIX_EPOCH + Duration::from_secs(90),
    );

    assert!(rendered.contains("url=https://youtu.be/abc123"));
    assert!(rendered.contains("utc=1970-01-01T00:01:30Z"));
    assert!(rendered.contains("unix=90"));
    assert!(!rendered.contains("{timestamp_local}"));
}
