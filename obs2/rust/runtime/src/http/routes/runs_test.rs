use super::*;

#[test]
fn missing_elite_users_map_to_not_found_without_masking_other_upstream_errors() {
    let missing = anyhow::Error::new(crate::run_library::elite::UserNotFound::new("missing-runner"));
    assert_eq!(elite_fetch_error_status(&missing), StatusCode::NOT_FOUND);
    assert_eq!(missing.to_string(), "The Elite user ~missing-runner was not found");

    let upstream = anyhow::anyhow!("The Elite returned 503 Service Unavailable");
    assert_eq!(elite_fetch_error_status(&upstream), StatusCode::BAD_GATEWAY);
}
