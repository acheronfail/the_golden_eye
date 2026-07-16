use super::*;

#[test]
fn due_check_respects_interval() {
    assert!(is_check_due(UpdateCheckInterval::Daily, None, 100));
    assert!(is_check_due(UpdateCheckInterval::Daily, Some(0), 24 * 60 * 60));
    assert!(!is_check_due(UpdateCheckInterval::Daily, Some(1), 24 * 60 * 60));
    assert!(!is_check_due(UpdateCheckInterval::Never, None, 100));
}

#[test]
fn release_newer_than_current_is_reported() {
    let update = update_from_release(
        "1.2.3",
        GithubRelease {
            tag_name: "v1.3.0".to_owned(),
            html_url: "https://github.com/acheronfail/the_golden_eye/releases/tag/v1.3.0".to_owned(),
            assets: Vec::new(),
        },
    )
    .unwrap()
    .unwrap();

    assert_eq!(update.latest_version, "v1.3.0");
}

#[test]
fn release_not_newer_than_current_is_ignored() {
    let update = update_from_release(
        "1.2.3",
        GithubRelease {
            tag_name: "v1.2.3".to_owned(),
            html_url: "https://github.com/acheronfail/the_golden_eye/releases/tag/v1.2.3".to_owned(),
            assets: Vec::new(),
        },
    )
    .unwrap();

    assert!(update.is_none());
}

#[test]
fn release_url_validation_is_repo_scoped() {
    assert!(is_allowed_release_url("https://github.com/acheronfail/the_golden_eye/releases/tag/v1.2.3"));
    assert!(!is_allowed_release_url("https://github.com/acheronfail/other/releases/tag/v1.2.3"));
    assert!(!is_allowed_release_url("https://example.com/acheronfail/the_golden_eye/releases/tag/v1.2.3"));
}
