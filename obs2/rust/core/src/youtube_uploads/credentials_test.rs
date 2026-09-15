use super::*;

#[test]
fn keyring_store_round_trips_tokens() {
    let suffix = format!("{}-{}", std::process::id(), unix_secs(SystemTime::now()));
    let store = KeyringYoutubeCredentialStore::test_account(&suffix);
    let _ = store.delete();
    let tokens = YoutubeTokens {
        refresh_token: "refresh".to_owned(),
        access_token: Some("access".to_owned()),
        expires_at_unix_secs: Some(123),
        scope: Some(YOUTUBE_UPLOAD_SCOPE.to_owned()),
        token_type: Some("Bearer".to_owned()),
        account: Some(YoutubeAccount {
            email: Some("test@example.com".to_owned()),
            name: Some("Test User".to_owned()),
            picture: None,
        }),
    };
    store.save(&tokens).expect("save tokens");
    assert_eq!(store.load().expect("load tokens"), Some(tokens));
    store.delete().expect("delete tokens");
    assert_eq!(store.load().expect("load deleted tokens"), None);
}

#[test]
fn fallback_store_uses_file_when_primary_fails() {
    let dir = crate::config::temp_dir().join(format!("ge-youtube-fallback-{}", std::process::id()));
    let path = dir.join("tokens.json");
    let _ = fs::remove_file(&path);
    let store = FallbackYoutubeCredentialStore {
        primary: Arc::new(FailingYoutubeCredentialStore),
        file: FileYoutubeCredentialStore { path: path.clone() },
    };
    let tokens = YoutubeTokens {
        refresh_token: "refresh".to_owned(),
        access_token: Some("access".to_owned()),
        expires_at_unix_secs: Some(123),
        scope: Some(YOUTUBE_UPLOAD_SCOPE.to_owned()),
        token_type: Some("Bearer".to_owned()),
        account: None,
    };

    store.save(&tokens).expect("save fallback tokens");
    assert_eq!(store.load().expect("load fallback tokens"), Some(tokens));
    store.delete().expect("delete fallback tokens");
    assert_eq!(store.load().expect("load deleted fallback tokens"), None);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn oauth_url_contains_required_parameters() {
    let config = YoutubeConfig {
        client_id: "client".to_owned(),
        client_secret: String::new(),
        auth_url: "https://example.test/auth".to_owned(),
        token_url: "https://example.test/token".to_owned(),
        upload_url: "https://example.test/upload".to_owned(),
        userinfo_url: "https://example.test/userinfo".to_owned(),
        redirect_uri: config::loopback_http_url(crate::youtube_uploads::OAUTH_CALLBACK_PATH),
        scope: YOUTUBE_UPLOAD_SCOPE.to_owned(),
        enabled: true,
    };
    let url = config.authorization_url("state-123");
    assert!(url.contains("client_id=client"));
    assert!(url.contains("access_type=offline"));
    assert!(url.contains("prompt=consent"));
    assert!(url.contains("state=state-123"));
    assert!(url.contains("youtube.upload"));
}
