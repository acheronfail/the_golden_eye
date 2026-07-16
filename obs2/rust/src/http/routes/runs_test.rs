use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

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
            let path = std::env::temp_dir().join(format!("ge-runs-{label}-{}-{nanos}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return TestDir { path },
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
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
fn normalize_time_formats_mm_ss_and_seconds() {
    assert_eq!(normalize_time("1:02").unwrap(), Some((62, "01:02".to_owned())));
    assert_eq!(normalize_time("12:34").unwrap(), Some((754, "12:34".to_owned())));
    assert_eq!(normalize_time(" ").unwrap(), None);
}

#[test]
fn normalize_time_rejects_bad_values() {
    assert!(matches!(normalize_time("1"), Err(RunPathError::BadRequest(_))));
    assert!(matches!(normalize_time("1:2"), Err(RunPathError::BadRequest(_))));
    assert!(matches!(normalize_time("1:60"), Err(RunPathError::BadRequest(_))));
}

#[test]
fn normalized_run_file_name_preserves_extension_when_missing() {
    let path = Path::new("/runs/original.mov");
    assert_eq!(normalized_run_file_name(path, "renamed").unwrap(), "renamed.mov");
    assert_eq!(normalized_run_file_name(path, "renamed.mp4").unwrap(), "renamed.mp4");
}

#[test]
fn normalized_run_file_name_rejects_paths_and_non_video_extensions() {
    let path = Path::new("/runs/original.mov");
    assert!(matches!(normalized_run_file_name(path, "../renamed.mov"), Err(RunPathError::BadRequest(_))));
    assert!(matches!(normalized_run_file_name(path, "renamed.txt"), Err(RunPathError::BadRequest(_))));
}

#[test]
fn video_files_in_directory_searches_recursively() {
    let dir = TestDir::new("recursive-video-files");
    let nested = dir.join("Surface 2/00 Agent");
    fs::create_dir_all(&nested).unwrap();
    let root_clip = dir.join("root.mov");
    let nested_clip = nested.join("02-03.mp4");
    let ignored = nested.join("notes.txt");
    fs::write(&root_clip, b"root").unwrap();
    fs::write(&nested_clip, b"nested").unwrap();
    fs::write(&ignored, b"ignored").unwrap();

    let files = video_files_in_directory_recursive(&dir.path).unwrap();

    let mut expected = vec![root_clip, nested_clip];
    expected.sort();
    assert_eq!(files, expected);
}

#[test]
fn list_configured_runs_creates_missing_output_directories_before_scanning() {
    let dir = TestDir::new("configured-missing");
    let completed = dir.join("completed/deeply/nested");
    let failed = dir.join("failed/deeply/nested");
    let settings = AppSettings {
        completed_output_path: completed.to_string_lossy().into_owned(),
        save_failed_runs: true,
        failed_output_path: failed.to_string_lossy().into_owned(),
        ..AppSettings::default()
    };

    let runs = list_configured_runs(&settings);

    assert!(completed.is_dir());
    assert!(failed.is_dir());
    assert!(runs.clips.is_empty());
    assert_eq!(runs.directories.len(), 2);
    assert_eq!(runs.directories[0].kind, RunDirectoryKind::Completed);
    assert_eq!(runs.directories[0].path, completed.to_string_lossy());
    assert!(runs.directories[0].exists);
    assert_eq!(runs.directories[0].error, None);
    assert_eq!(runs.directories[1].kind, RunDirectoryKind::Failed);
    assert_eq!(runs.directories[1].path, failed.to_string_lossy());
    assert!(runs.directories[1].exists);
    assert_eq!(runs.directories[1].error, None);
}
