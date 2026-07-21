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
    let update = select_update_from_releases("1.2.3", vec![release("v1.3.0", false, false)], false).unwrap().unwrap().0;

    assert_eq!(update.latest_version, "v1.3.0");
}

#[test]
fn release_not_newer_than_current_is_ignored() {
    let update = select_update_from_releases("1.2.3", vec![release("v1.2.3", false, false)], false).unwrap();

    assert!(update.is_none());
}

#[test]
fn stable_selection_ignores_prereleases_and_drafts() {
    let found = select_update_from_releases(
        "1.2.3",
        vec![release("v1.4.0-beta.1", true, false), release("v1.5.0", false, true), release("v1.3.0", false, false)],
        false,
    )
    .unwrap()
    .unwrap()
    .0;

    assert_eq!(found.latest_version, "v1.3.0");
}

#[test]
fn prerelease_selection_includes_prereleases_but_not_drafts() {
    let found = select_update_from_releases(
        "1.2.3",
        vec![release("v1.3.0", false, false), release("v1.4.0-beta.1", true, false), release("v1.5.0", false, true)],
        true,
    )
    .unwrap()
    .unwrap()
    .0;

    assert_eq!(found.latest_version, "v1.4.0-beta.1");
}

#[test]
fn release_selection_chooses_highest_newer_semver() {
    let found = select_update_from_releases(
        "1.2.3",
        vec![release("v1.3.0", false, false), release("v1.4.0", false, false)],
        false,
    )
    .unwrap()
    .unwrap()
    .0;

    assert_eq!(found.latest_version, "v1.4.0");
}

#[test]
fn release_response_parses_single_release_object() {
    let value = serde_json::json!({
        "tag_name": "v1.3.0",
        "html_url": "https://github.com/acheronfail/the_golden_eye/releases/tag/v1.3.0"
    });

    let releases = releases_from_response(value).unwrap();

    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].tag_name, "v1.3.0");
}

#[test]
fn release_response_parses_release_array() {
    let value = serde_json::json!([
        {
            "tag_name": "v1.3.0",
            "html_url": "https://github.com/acheronfail/the_golden_eye/releases/tag/v1.3.0"
        },
        {
            "tag_name": "v1.4.0-beta.1",
            "html_url": "https://github.com/acheronfail/the_golden_eye/releases/tag/v1.4.0-beta.1",
            "prerelease": true
        }
    ]);

    let releases = releases_from_response(value).unwrap();

    assert_eq!(releases.len(), 2);
    assert_eq!(releases[1].tag_name, "v1.4.0-beta.1");
    assert!(releases[1].prerelease);
}

#[test]
fn env_config_defaults_to_latest_release_url() {
    let config = crate::config::UpdateEnvConfig::from_values(None, None);

    assert_eq!(config.releases_api_url(), crate::config::LATEST_RELEASE_API_URL);
    assert!(!config.include_prereleases());
}

#[test]
fn env_config_truthy_prereleases_uses_full_releases_url() {
    let config = crate::config::UpdateEnvConfig::from_values(None, Some("true".to_owned()));

    assert_eq!(config.releases_api_url(), crate::config::RELEASES_API_URL);
    assert!(config.include_prereleases());
}

#[test]
fn env_config_url_override_takes_precedence_over_prerelease_endpoint() {
    let config = crate::config::UpdateEnvConfig::from_values(
        Some("https://example.test/releases".to_owned()),
        Some("true".to_owned()),
    );

    assert_eq!(config.releases_api_url(), "https://example.test/releases");
    assert!(config.include_prereleases());
}

#[test]
fn env_config_false_prerelease_override_is_recorded_but_disabled() {
    let config = crate::config::UpdateEnvConfig::from_values(None, Some("false".to_owned()));

    assert_eq!(config.include_prereleases_override, Some(false));
    assert!(!config.include_prereleases());
    assert_eq!(config.releases_api_url(), crate::config::LATEST_RELEASE_API_URL);
}

#[test]
fn env_value_enabled_accepts_common_truthy_values() {
    for value in ["1", "true", "TRUE", " yes ", "on"] {
        assert!(crate::config::EnvVar::truthy_value(value));
    }
    for value in ["", "0", "false", "no", "off", "anything"] {
        assert!(!crate::config::EnvVar::truthy_value(value));
    }
}

#[test]
fn release_url_validation_is_repo_scoped() {
    assert!(is_allowed_release_url("https://github.com/acheronfail/the_golden_eye/releases/tag/v1.2.3"));
    assert!(!is_allowed_release_url("https://github.com/acheronfail/other/releases/tag/v1.2.3"));
    assert!(!is_allowed_release_url("https://example.com/acheronfail/the_golden_eye/releases/tag/v1.2.3"));
}

fn release(tag_name: &str, prerelease: bool, draft: bool) -> GithubRelease {
    GithubRelease {
        tag_name: tag_name.to_owned(),
        html_url: format!("https://github.com/acheronfail/the_golden_eye/releases/tag/{tag_name}"),
        prerelease,
        draft,
        assets: Vec::new(),
    }
}
