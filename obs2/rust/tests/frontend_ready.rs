mod support;

use std::time::Duration;

use support::harness::{API, Harness, SOURCE_NAME};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn startup_only_queries_dock_config_before_frontend_ready() {
    let harness = Harness::start_before_frontend_ready(Duration::ZERO).await;

    ge_rust::ge_sources_changed();
    ge_rust::ge_replay_buffer_starting();
    ge_rust::ge_replay_buffer_started();
    ge_rust::ge_replay_buffer_stopping();
    ge_rust::ge_replay_buffer_stopped();

    let calls = harness.obs.calls();
    assert_eq!(
        calls.runtime_frontend_queries(),
        0,
        "startup and pre-ready lifecycle callbacks must not query runtime OBS frontend/source APIs"
    );
    assert_eq!(calls.dock_config_queries(), 0, "dock config setup should wait for module post-load");

    ge_rust::ge_browser_dock_post_load();

    let calls = harness.obs.calls();
    assert_eq!(
        calls.runtime_frontend_queries(),
        0,
        "browser dock post-load must not query runtime OBS frontend/source APIs"
    );
    assert!(calls.dock_config_queries() > 0, "browser dock post-load should ensure the OBS dock config");
    assert!(harness.obs.dock_json().contains("thegoldeneyedashboard"));

    harness.mark_frontend_ready();

    let calls = harness.obs.calls();
    assert!(calls.source_names > 0, "frontend-ready should perform the first source refresh");
    assert!(calls.replay_enabled > 0, "frontend-ready should perform the first replay-buffer status refresh");

    let sources: serde_json::Value =
        harness.client.get(format!("{API}/api/v1/sources")).send().await.unwrap().json().await.unwrap();
    assert_eq!(sources, serde_json::json!([{"name":SOURCE_NAME,"id":"test_input"}]));
}
