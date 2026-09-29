use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

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
