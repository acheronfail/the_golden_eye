use ge_clip::RunStatus;

use super::*;

#[test]
fn legacy_youtube_metadata_defaults_to_plugin_upload_source() {
    let metadata: YoutubeMetadata = serde_json::from_str(
        r#"{
                "videoId": "legacy-id",
                "videoUrl": "https://youtu.be/legacy-id",
                "uploadedAt": "2026-01-01T00:00:00Z",
                "title": "Legacy upload"
            }"#,
    )
    .expect("deserialize legacy metadata");

    assert_eq!(metadata.source, YoutubeAssociationSource::PluginUpload);
    assert_eq!(metadata.uploaded_at.as_deref(), Some("2026-01-01T00:00:00Z"));
}

#[test]
fn external_youtube_metadata_omits_unknown_upload_time() {
    let metadata = YoutubeMetadata {
        video_id: "external-id".to_owned(),
        video_url: "https://youtu.be/external-id".to_owned(),
        uploaded_at: None,
        title: "External video".to_owned(),
        source: YoutubeAssociationSource::ManualLink,
    };

    let json = serde_json::to_value(metadata).expect("serialize external metadata");
    assert_eq!(json["source"], "manualLink");
    assert!(json.get("uploadedAt").is_none());
}

fn clip_metadata_fixture() -> ClipMetadata {
    ClipMetadata {
        run_id: "run-1".to_owned(),
        timestamp: "2026-07-18T10:30:45Z".to_owned(),
        time: Some("01:23".to_owned()),
        time_seconds: Some(83),
        level: "Dam".to_owned(),
        level_number: Some(1),
        difficulty: Some("Agent".to_owned()),
        status: RunStatus::Complete,
        was_personal_best: false,
        game_language: "en".to_owned(),
        rom_version: None,
        source_name: "N64 Capture".to_owned(),
        comment: "test".to_owned(),
        plugin_version: "test".to_owned(),
        retention_state: "kept".to_owned(),
        retention_reason: None,
    }
}

#[test]
fn renderer_uses_browser_datetime_local_token() {
    let mut settings = AppSettings::default();
    settings.youtube_title_template = "{level} at {datetime_local}".to_owned();
    settings.youtube_description_template = "Achieved at {datetime_local}".to_owned();
    let metadata = clip_metadata_fixture();

    let (title, description) =
        render_youtube_metadata(&settings, Path::new("clip.mov"), &metadata, Some("7/18/2026, 3:30:45 AM"));

    assert_eq!(title, "Dam at 7/18/2026, 3:30:45 AM");
    assert_eq!(description, "Achieved at 7/18/2026, 3:30:45 AM");
}

#[test]
fn renderer_falls_back_to_timestamp_local_for_datetime_local() {
    let mut settings = AppSettings::default();
    settings.youtube_title_template = "{level}".to_owned();
    settings.youtube_description_template = "Achieved at {datetime_local}".to_owned();
    let metadata = clip_metadata_fixture();
    let expected_local = RunTemplateTokens::from_clip_metadata("clip", &metadata).timestamp_local;

    let (_title, description) = render_youtube_metadata(&settings, Path::new("clip.mov"), &metadata, None);

    assert_eq!(description, format!("Achieved at {expected_local}"));
}

#[test]
fn renderer_falls_back_to_timestamp_local_for_blank_datetime_local() {
    let mut settings = AppSettings::default();
    settings.youtube_title_template = "{level}".to_owned();
    settings.youtube_description_template = "Achieved at {datetime_local}".to_owned();
    let metadata = clip_metadata_fixture();
    let expected_local = RunTemplateTokens::from_clip_metadata("clip", &metadata).timestamp_local;

    let (_title, description) = render_youtube_metadata(&settings, Path::new("clip.mov"), &metadata, Some("   "));

    assert_eq!(description, format!("Achieved at {expected_local}"));
}

fn upload_store() -> YoutubeUploadStore {
    YoutubeUploadStore::with_parts(
        Arc::new(RunCatalog::open(":memory:".into()).unwrap()),
        Arc::new(credentials::MemoryYoutubeCredentialStore::default()),
        YoutubeConfig::from_env(),
    )
}

fn queued_upload(store: &YoutubeUploadStore) -> YoutubeUploadStatus {
    store.insert_queued_upload(
        Path::new("clip.mov"),
        "run-1".into(),
        "clip.mov".into(),
        "Run".into(),
        String::new(),
        100,
    )
}

#[tokio::test]
async fn cancellation_releases_queued_upload_and_allows_distinct_retry() {
    let store = upload_store();
    let occupied = store.semaphore().acquire_many_owned(UPLOAD_CONCURRENCY as u32).await.unwrap();
    let status = queued_upload(&store);
    let cancellation = store.cancellation(&status.id);
    let (events, mut receiver) = tokio::sync::broadcast::channel(8);
    let request = upload::UploadRequest {
        upload_id: status.id.clone(),
        run_id: status.run_id.clone(),
        path: "clip.mov".into(),
        title: "Run".into(),
        description: String::new(),
        visibility: crate::settings::YoutubeVisibility::Private,
    };
    let task = tokio::spawn(upload::upload_video(store.clone(), request, events.clone(), cancellation));
    assert_eq!(store.cancel_upload(&status.id, &events).unwrap().state, YoutubeUploadState::Cancelling);
    tokio::time::timeout(Duration::from_secs(1), task).await.unwrap().unwrap();
    assert_eq!(store.uploads()[0].state, YoutubeUploadState::Cancelled);
    assert!(store.uploads()[0].finished_at.is_some());
    assert!(store.active_upload_for_display_path("clip.mov").is_none());
    assert!(store.inner.lock().unwrap().cancellations.is_empty());
    assert!(store.read_history().is_empty());
    assert!(
        matches!(receiver.recv().await.unwrap(), crate::app::AppEvent::YoutubeUploadChanged { upload } if upload.state == YoutubeUploadState::Cancelling)
    );
    assert!(
        matches!(receiver.recv().await.unwrap(), crate::app::AppEvent::YoutubeUploadChanged { upload } if upload.state == YoutubeUploadState::Cancelled)
    );
    let retry = queued_upload(&store);
    assert_ne!(status.id, retry.id);
    assert!(store.update_upload(&status.id, |upload| upload.state = YoutubeUploadState::Uploading).is_none());
    assert_eq!(store.active_upload_for_display_path("clip.mov").unwrap().id, retry.id);
    drop(occupied);
    assert_eq!(store.semaphore().available_permits(), UPLOAD_CONCURRENCY);
}

#[test]
fn cancellation_is_idempotent_and_does_not_overwrite_completion() {
    let store = upload_store();
    let status = queued_upload(&store);
    let (events, _) = tokio::sync::broadcast::channel(8);
    assert!(store.cancel_upload("missing", &events).is_none());
    for _ in 0..2 {
        assert_eq!(store.cancel_upload(&status.id, &events).unwrap().state, YoutubeUploadState::Cancelling);
    }
    assert!(store.update_upload(&status.id, |upload| upload.state = YoutubeUploadState::Uploading).is_none());
    store.update_upload(&status.id, |upload| upload.state = YoutubeUploadState::Uploaded).unwrap();
    assert_eq!(store.cancel_upload(&status.id, &events).unwrap().state, YoutubeUploadState::Uploaded);
    assert!(store.update_upload(&status.id, |upload| upload.state = YoutubeUploadState::Cancelled).is_none());
}

#[test]
fn rapid_retries_have_unique_ids_even_at_the_same_timestamp() {
    let now = SystemTime::now();
    assert_ne!(upload_id(Path::new("clip.mov"), now), upload_id(Path::new("clip.mov"), now));
}
