use super::*;

#[test]
fn current_platform_is_supported_when_the_arch_can_be_packaged() {
    assert_eq!(platform_arch_suffix().is_some(), matches!(std::env::consts::ARCH, "aarch64" | "x86_64"));
}

#[test]
fn asset_zip_names_match_the_package_contract() {
    let cases = [
        ("v1.2.3", "macos", "aarch64", "the_golden_eye-u4-v1.2.3-macos-arm64.zip"),
        ("1.2.3", "macos", "x86_64", "the_golden_eye-u4-v1.2.3-macos-x86_64.zip"),
        ("v1.2.3-beta.1", "linux", "x86_64", "the_golden_eye-u4-v1.2.3-beta.1-linux-x86_64.zip"),
        ("v1.2.3+build.4", "linux", "aarch64", "the_golden_eye-u4-v1.2.3+build.4-linux-arm64.zip"),
        ("v1.2.3", "windows", "x86_64", "the_golden_eye-u4-v1.2.3-windows-x86_64.zip"),
    ];

    for (version, os, arch, expected) in cases {
        assert_eq!(asset_zip_name_for(version, 4, os, arch).as_deref(), Some(expected));
    }
    assert_eq!(asset_zip_name_for("v1.2.3", 4, "windows", "aarch64"), None);
    assert_eq!(asset_zip_name_for("v1.2.3", 4, "freebsd", "x86_64"), None);
}

#[test]
fn update_safety_only_blocks_monitoring_and_in_flight_recording() {
    assert!(activity_is_safe_to_apply(false, false));
    if cfg!(feature = "dev") {
        assert!(activity_is_safe_to_apply(true, false));
        assert!(activity_is_safe_to_apply(false, true));
        assert!(activity_is_safe_to_apply(true, true));
    } else {
        assert!(!activity_is_safe_to_apply(true, false));
        assert!(!activity_is_safe_to_apply(false, true));
        assert!(!activity_is_safe_to_apply(true, true));
    }
}

#[test]
fn find_named_searches_nested_directories() {
    let dir = tempdir();
    std::fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
    std::fs::write(dir.path().join("a/b/c/target.txt"), b"hi").unwrap();

    let found = find_named(dir.path(), OsStr::new("target.txt"));
    assert_eq!(found, Some(dir.path().join("a/b/c/target.txt")));
    assert_eq!(find_named(dir.path(), OsStr::new("missing.txt")), None);
}

#[test]
fn copy_dir_recursive_preserves_structure() {
    let dir = tempdir();
    let src = dir.path().join("src");
    std::fs::create_dir_all(src.join("nested")).unwrap();
    std::fs::write(src.join("nested/file.txt"), b"hi").unwrap();

    let dst = dir.path().join("dst");
    copy_dir_recursive(&src, &dst).unwrap();
    assert_eq!(std::fs::read(dst.join("nested/file.txt")).unwrap(), b"hi");
}

#[test]
fn packaged_core_is_staged_under_the_installed_custom_name() {
    let dir = tempdir();
    let extracted = dir.path().join("extracted/package/nested");
    std::fs::create_dir_all(extracted.join("cv_templates")).unwrap();
    std::fs::create_dir_all(extracted.join("locale")).unwrap();
    std::fs::write(extracted.join(packaged_core_name()), b"core").unwrap();
    std::fs::write(extracted.join("cv_templates/template.png"), b"template").unwrap();
    std::fs::write(extracted.join("locale/en-US.ini"), b"locale").unwrap();

    let prepared = dir.path().join("prepared");
    prepare_staged_update(&dir.path().join("extracted"), &prepared, OsStr::new("custom core name.test")).unwrap();

    assert_eq!(std::fs::read(prepared.join("custom core name.test")).unwrap(), b"core");
    assert_eq!(std::fs::read(prepared.join("cv_templates/template.png")).unwrap(), b"template");
    assert_eq!(std::fs::read(prepared.join("locale/en-US.ini")).unwrap(), b"locale");
}

#[test]
fn prepared_update_requires_a_fresh_destination_and_all_runtime_data() {
    let dir = tempdir();
    let extracted = dir.path().join("extracted");
    std::fs::create_dir_all(extracted.join("cv_templates")).unwrap();
    std::fs::write(extracted.join(packaged_core_name()), b"core").unwrap();

    let prepared = dir.path().join("prepared");
    let error = prepare_staged_update(&extracted, &prepared, OsStr::new("custom-core")).unwrap_err();
    assert!(error.to_string().contains("locale"));

    std::fs::remove_dir_all(&prepared).unwrap();
    std::fs::create_dir_all(extracted.join("locale")).unwrap();
    std::fs::create_dir(&prepared).unwrap();
    std::fs::write(prepared.join("stale.txt"), b"stale").unwrap();
    let error = prepare_staged_update(&extracted, &prepared, OsStr::new("custom-core")).unwrap_err();
    assert!(error.to_string().contains("fresh prepared update directory"));
}

#[test]
fn update_workspaces_are_unique_and_start_empty() {
    let dir = tempdir();
    let staged = dir.path().join(".ge_update_staged");
    let first = UpdateWorkDir::create(&staged).unwrap();
    std::fs::write(first.path().join("stale.txt"), b"stale").unwrap();
    let first_path = first.path().to_owned();

    let second = UpdateWorkDir::create(&staged).unwrap();
    assert_ne!(first.path(), second.path());
    assert_eq!(std::fs::read_dir(second.path()).unwrap().count(), 0);
    let second_path = second.path().to_owned();

    drop(first);
    drop(second);
    assert!(!first_path.exists());
    assert!(!second_path.exists());
}

#[test]
fn staged_publication_refuses_a_non_directory_destination() {
    let dir = tempdir();
    let staged = dir.path().join(".ge_update_staged");
    std::fs::write(&staged, b"not a directory").unwrap();

    assert!(remove_staged_dir(&staged).is_err());
    assert_eq!(std::fs::read(&staged).unwrap(), b"not a directory");
}

#[test]
fn runtime_data_commit_keeps_new_directories() {
    let dir = tempdir();
    let staged = dir.path().join("staging on another path");
    let data = dir.path().join("OBS data with spaces");
    seed_runtime_data(&staged, "new");
    seed_runtime_data(&data, "old");

    let transaction = RuntimeDataTransaction::install(&staged, &data).unwrap();
    assert_runtime_data(&data, "new");
    transaction.commit();

    assert_runtime_data(&data, "new");
    assert!(
        std::fs::read_dir(&data).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".ge-update-"))
    );
}

#[test]
fn runtime_data_startup_failure_restores_old_directories() {
    let dir = tempdir();
    let staged = dir.path().join("unrelated staging");
    let data = dir.path().join("unrelated data");
    seed_runtime_data(&staged, "new");
    seed_runtime_data(&data, "old");

    {
        let _transaction = RuntimeDataTransaction::install(&staged, &data).unwrap();
        assert_runtime_data(&data, "new");
    }

    assert_runtime_data(&data, "old");
}

fn seed_runtime_data(root: &Path, content: &str) {
    for name in RUNTIME_DATA_DIRS {
        std::fs::create_dir_all(root.join(name)).unwrap();
        std::fs::write(root.join(name).join("marker.txt"), content).unwrap();
    }
}

fn assert_runtime_data(root: &Path, content: &str) {
    for name in RUNTIME_DATA_DIRS {
        assert_eq!(std::fs::read_to_string(root.join(name).join("marker.txt")).unwrap(), content);
    }
}

fn tempdir() -> TestDir {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("ge-update-apply-test-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    TestDir(path)
}

struct TestDir(PathBuf);
impl TestDir {
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
