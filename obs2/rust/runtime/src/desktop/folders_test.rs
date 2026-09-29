use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

#[test]
fn queued_picker_is_skipped_after_shutdown_even_if_the_request_was_dropped() {
    let lifecycle = crate::app::lifecycle::CoreLifecycle::default();
    let mut queued = None;
    let result = queue_folder_picker(
        lifecycle.clone(),
        || panic!("a cancelled queued dialog must not open"),
        |task| {
            queued = Some(task);
            Ok(())
        },
    )
    .unwrap();
    assert!(!lifecycle.begin_update(false));
    drop(result);
    lifecycle.close();
    queued.unwrap()();
}

#[test]
fn open_picker_keeps_update_blocked_after_the_http_request_is_dropped() {
    let lifecycle = crate::app::lifecycle::CoreLifecycle::default();
    let (opened_tx, opened_rx) = std::sync::mpsc::channel();
    let (close_tx, close_rx) = std::sync::mpsc::channel();
    let mut native_thread = None;
    let result = queue_folder_picker(
        lifecycle.clone(),
        move || {
            opened_tx.send(()).unwrap();
            close_rx.recv().unwrap();
            None
        },
        |task| {
            native_thread = Some(std::thread::spawn(task));
            Ok(())
        },
    )
    .unwrap();
    opened_rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    drop(result);
    assert!(!lifecycle.begin_update(false));
    close_tx.send(()).unwrap();
    native_thread.unwrap().join().unwrap();
    assert!(lifecycle.begin_update(false));
}

#[test]
fn failed_picker_dispatch_releases_update_gate() {
    let lifecycle = crate::app::lifecycle::CoreLifecycle::default();
    assert!(queue_folder_picker(lifecycle.clone(), || None, |_| anyhow::bail!("pin failed")).is_err());
    assert!(lifecycle.begin_update(false));
}

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(label: &str) -> Self {
        loop {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!("ge-folders-{label}-{}-{nanos}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return TestDir { path },
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create test dir {}: {err}", path.display()),
            }
        }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn validate_accepts_existing_writable_directory() {
    let dir = TestDir::new("existing");
    let validation = validate_folder_path(&dir.path.to_string_lossy());

    assert!(validation.exists);
    assert!(validation.is_directory);
    assert!(validation.writable);
    assert!(!validation.will_create);
    assert_eq!(validation.error, None);
}

#[test]
fn validate_allows_missing_child_when_parent_is_writable() {
    let dir = TestDir::new("missing");
    let validation = validate_folder_path(&dir.join("child/grandchild").to_string_lossy());

    assert!(!validation.exists);
    assert!(validation.writable);
    assert!(validation.will_create);
    assert_eq!(validation.error, None);
}

#[test]
fn validate_rejects_existing_file() {
    let dir = TestDir::new("file");
    let file = dir.join("clip.mp4");
    fs::write(&file, b"clip").unwrap();

    let validation = validate_folder_path(&file.to_string_lossy());

    assert!(validation.exists);
    assert!(!validation.is_directory);
    assert!(!validation.writable);
    assert_eq!(validation.error, Some("Path exists but is not a folder.".to_owned()));
}
