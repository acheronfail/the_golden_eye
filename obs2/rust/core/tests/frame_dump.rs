use std::time::Duration;

use crate::support::harness::{API, Harness, SOURCE_NAME};
use crate::support::test_obs::TestObs;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn core_shutdown_joins_standalone_frame_dump() {
    let harness = Harness::start(Duration::ZERO).await;
    harness
        .client
        .post(format!("{API}/api/v1/monitor/frame-dump"))
        .json(&serde_json::json!({ "enabled": true, "source": SOURCE_NAME }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert_eq!(harness.obs.calls().frame_callback_register, 1);
    assert_eq!(harness.obs.calls().frame_callback_unregister, 0);

    drop(harness);
    let calls = TestObs.calls();
    assert_eq!(calls.frame_callback_unregister, 1);
    assert_eq!(calls.capture_destroy, 1);
}
