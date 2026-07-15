use super::*;

#[test]
fn current_platform_is_supported_when_the_arch_can_be_packaged() {
    assert_eq!(platform_arch_suffix().is_some(), matches!(std::env::consts::ARCH, "aarch64" | "x86_64"));
}

#[test]
fn asset_zip_names_match_the_package_contract() {
    let cases = [
        ("v1.2.3", "macos", "aarch64", "the_golden_eye-1.2.3-macos-arm64.zip"),
        ("1.2.3", "macos", "x86_64", "the_golden_eye-1.2.3-macos-x86_64.zip"),
        ("v1.2.3-beta.1", "linux", "x86_64", "the_golden_eye-1.2.3-beta.1-linux-x86_64.zip"),
        ("v1.2.3+build.4", "linux", "aarch64", "the_golden_eye-1.2.3+build.4-linux-arm64.zip"),
        ("v1.2.3", "windows", "x86_64", "the_golden_eye-1.2.3-windows-x86_64.zip"),
    ];

    for (version, os, arch, expected) in cases {
        assert_eq!(asset_zip_name_for(version, os, arch).as_deref(), Some(expected));
    }
    assert_eq!(asset_zip_name_for("v1.2.3", "windows", "aarch64"), None);
    assert_eq!(asset_zip_name_for("v1.2.3", "freebsd", "x86_64"), None);
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
