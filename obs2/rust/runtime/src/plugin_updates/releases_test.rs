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
    assert_eq!(update.updater_version, installed_updater_version().unwrap());
    assert!(!update.requires_manual_install);
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
fn updater_version_mismatch_requires_manual_install() {
    let installed = installed_updater_version().unwrap();
    let target = installed + 1;
    let found = select_update_from_releases("1.2.3", vec![release_with_updater("v1.3.0", false, false, target)], false)
        .unwrap()
        .unwrap()
        .0;

    assert_eq!(found.updater_version, target);
    assert!(found.requires_manual_install);
}

#[test]
fn canonical_package_must_match_release_and_platform_exactly() {
    let release_version = Version::parse("1.3.0").unwrap();
    let updater = installed_updater_version().unwrap();
    let asset = |name: &str| GithubAsset { name: name.to_owned(), browser_download_url: String::new() };

    assert_eq!(
        updater_version_from_assets(
            &release_version,
            &[asset(&format!("the_golden_eye-u{updater}-v1.3.0-linux-x86_64.zip"))],
            "linux",
            "x86_64",
        )
        .unwrap(),
        updater
    );
    assert!(
        updater_version_from_assets(
            &release_version,
            &[asset("the_golden_eye-1.3.0-linux-x86_64.zip")],
            "linux",
            "x86_64",
        )
        .is_err()
    );
    assert!(
        updater_version_from_assets(
            &release_version,
            &[asset(&format!("the_golden_eye-u{updater}-v1.3.1-linux-x86_64.zip"))],
            "linux",
            "x86_64",
        )
        .is_err()
    );
}

#[test]
fn malformed_or_ambiguous_canonical_packages_fail_closed() {
    let release_version = Version::parse("1.3.0").unwrap();
    let asset = |name: &str| GithubAsset { name: name.to_owned(), browser_download_url: String::new() };

    assert!(
        updater_version_from_assets(
            &release_version,
            &[asset("the_golden_eye-unope-v1.3.0-linux-x86_64.zip")],
            "linux",
            "x86_64",
        )
        .is_err()
    );
    assert!(
        updater_version_from_assets(
            &release_version,
            &[asset("the_golden_eye-u1-v1.3.0-linux-x86_64.zip"), asset("the_golden_eye-u2-v1.3.0-linux-x86_64.zip"),],
            "linux",
            "x86_64",
        )
        .is_err()
    );
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
fn env_config_defaults_to_release_history_url() {
    let config = crate::config::UpdateEnvConfig::from_values(None, None);

    assert_eq!(config.releases_api_url(), crate::config::RELEASES_API_URL);
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
    assert_eq!(config.releases_api_url(), crate::config::RELEASES_API_URL);
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
    release_with_updater(tag_name, prerelease, draft, installed_updater_version().unwrap())
}

fn release_with_updater(tag_name: &str, prerelease: bool, draft: bool, updater_version: u32) -> GithubRelease {
    let version = tag_name.trim_start_matches('v');
    let asset_name = format!("the_golden_eye-u{updater_version}-v{version}-{}", current_platform_suffix());
    GithubRelease {
        tag_name: tag_name.to_owned(),
        html_url: format!("https://github.com/acheronfail/the_golden_eye/releases/tag/{tag_name}"),
        prerelease,
        draft,
        assets: vec![GithubAsset {
            name: asset_name,
            browser_download_url: "https://example.test/package.zip".to_owned(),
        }],
    }
}

fn current_platform_suffix() -> String {
    format!(
        "{}.zip",
        platform_arch_suffix_for(std::env::consts::OS, std::env::consts::ARCH)
            .expect("tests require a packaged platform")
    )
}

#[test]
fn compatible_upgrade_precedes_newer_manual_release() {
    let updater = installed_updater_version().unwrap();
    for current in ["0.19.0", "0.20.0", "0.21.0"] {
        let releases = vec![
            release_with_updater("v0.22.0", false, false, updater + 1),
            release("v0.21.0", false, false),
            release("v0.20.0", false, false),
        ];
        let update = select_update_from_releases(current, releases, false).unwrap().unwrap().0;
        let manual = current == "0.21.0";
        assert_eq!(update.latest_version, if manual { "v0.22.0" } else { "v0.21.0" });
        assert_eq!(update.requires_manual_install, manual);
    }
}

#[test]
fn unusable_history_does_not_hide_compatible_upgrade() {
    let mut missing = release("v0.24.0", false, false);
    missing.assets.clear();
    let mut malformed = release("v0.23.0", false, false);
    malformed.assets[0].name = "the_golden_eye-unope-v0.23.0-linux-x86_64.zip".into();
    let found = select_update_from_releases(
        "0.20.0",
        vec![
            missing,
            malformed,
            release("not-semver", false, false),
            release("v0.21.0", false, false),
            release("v0.25.0-beta.1", true, false),
            release("v0.26.0", false, true),
        ],
        false,
    )
    .unwrap()
    .unwrap()
    .0;
    assert_eq!(found.latest_version, "v0.21.0");
}

#[tokio::test]
async fn release_history_follows_pages_and_rejects_incomplete_history() {
    use axum::Router;
    use axum::response::IntoResponse;
    use axum::routing::get;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let next = format!("<{base}/last>; rel=\"next\", <{base}/last>; rel=\"last\"");
    let repeated = format!("<{base}/cycle>; rel=\"next\"");
    let failed = format!("<{base}/missing>; rel=\"next\"");
    let body = r#"[{"tag_name":"v0.21.0","html_url":"https://example.test"}]"#;
    let app = Router::new()
        .route("/first", get(move || async move { ([("link", next)], "[]").into_response() }))
        .route("/last", get(move || async move { body }))
        .route("/cycle", get(move || async move { ([("link", repeated)], "[]") }))
        .route("/failed", get(move || async move { ([("link", failed)], "[]") }));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let result = fetch_releases(&format!("{base}/first")).await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].tag_name, "v0.21.0");
    assert!(fetch_releases(&format!("{base}/cycle")).await.is_err());
    assert!(fetch_releases(&format!("{base}/failed")).await.is_err());
    server.abort();
}
