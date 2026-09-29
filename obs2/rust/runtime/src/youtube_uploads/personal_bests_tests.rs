use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::youtube_uploads::credentials::{MemoryYoutubeCredentialStore, YoutubeConfig};

struct Fixture {
    uploader: PersonalBestUploader,
    events: broadcast::Receiver<AppEvent>,
    dir: PathBuf,
}

impl Fixture {
    fn new(enabled: bool) -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("ge-pb-upload-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, format!(r#"{{"youtubeAutoUploadPersonalBests":{enabled}}}"#)).unwrap();
        let settings = Arc::new(SettingsStore::load_from_path(path));
        let uploads = YoutubeUploadStore::with_parts(
            Arc::new(ge_catalog::run_catalog::RunCatalog::open(":memory:".into()).unwrap()),
            Arc::new(MemoryYoutubeCredentialStore::default()),
            YoutubeConfig::from_env(),
        );
        let (event_tx, events) = broadcast::channel(8);
        Self { uploader: PersonalBestUploader::new(settings, uploads, event_tx), events, dir }
    }

    fn upload(&self) -> YoutubeUploadStatus {
        self.uploader.uploads.insert_queued_upload(
            Path::new("clip.mov"),
            "run-1".into(),
            "clip.mov".into(),
            "PB".into(),
            String::new(),
            100,
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn only_opted_in_personal_bests_reach_the_upload_queue() {
    for (enabled, personal_best) in [(false, true), (true, false), (false, false)] {
        let mut fixture = Fixture::new(enabled);
        fixture.uploader.queue_clip_with("clip.mov", "run-1", personal_best, || panic!("must not queue upload"));
        assert!(fixture.events.try_recv().is_err());
    }
}

#[tokio::test]
async fn newly_queued_personal_best_publishes_notification() {
    let mut fixture = Fixture::new(true);
    let upload = fixture.upload();
    fixture.uploader.queue_clip_with("clip.mov", "run-1", true, || Ok((upload.clone(), true)));
    let AppEvent::YoutubePersonalBestUploadStarted { upload: notified } = fixture.events.try_recv().unwrap() else {
        panic!("expected PB notification");
    };
    assert_eq!(notified.id, upload.id);
    assert!(fixture.events.try_recv().is_err());
}

#[tokio::test]
async fn existing_upload_and_queue_errors_do_not_publish_pb_notification() {
    let mut fixture = Fixture::new(true);
    let upload = fixture.upload();
    fixture.uploader.queue_clip_with("clip.mov", "run-1", true, || Ok((upload, false)));
    for error in [QueueError::Disabled, QueueError::Disconnected, QueueError::File] {
        fixture.uploader.queue_clip_with("clip.mov", "run-1", true, || Err(error));
    }
    assert!(fixture.events.try_recv().is_err());
}
