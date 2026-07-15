mod support;

use std::time::Duration;

use serde_json::Value;
use support::harness::{API, Harness};

/// The developer "match a frame from disk" endpoint decodes an uploaded image and
/// returns the match plus the digit-slot diagnostics. Uses a committed flicker
/// fixture (a real dumped frame that misread the best time before the fix).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn match_upload_reads_a_dropped_frame_with_diagnostics() {
    let harness = Harness::start(Duration::ZERO).await;
    let fixture =
        harness.root.join("test/screenshots-rt4kce/en - stats - 3 - Agent - 0028_0500_0028 - flicker-004.png");
    let bytes = std::fs::read(&fixture).expect("read fixture");

    let body: Value = harness
        .client
        .post(format!("{API}/api/v1/match/upload?lang=en&annotations=true"))
        .body(bytes)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(body["match"]["screen"], "Stats");
    assert_eq!(body["match"]["times"]["best_time"], 28, "best time reads correctly through the upload path");
    assert!(body["frameWidth"].as_u64().unwrap() > 0);
    assert!(body["frameHeight"].as_u64().unwrap() > 0);

    // The digit-slot diagnostics set is present so the dev overlay can show it.
    let sets = body["match"]["annotation_sets"].as_array().expect("annotation sets");
    assert!(sets.iter().any(|set| set["id"] == "time_digits"), "expected a `time_digits` annotation set, got {sets:?}");
}

/// A non-image body is rejected rather than panicking the matcher.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn match_upload_rejects_a_non_image_body() {
    let harness = Harness::start(Duration::ZERO).await;
    let status = harness
        .client
        .post(format!("{API}/api/v1/match/upload?lang=en&annotations=false"))
        .body(b"not an image".to_vec())
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}
