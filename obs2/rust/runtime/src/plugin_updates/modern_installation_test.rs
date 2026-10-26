use super::*;

struct TestDir(std::path::PathBuf);
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

fn seed() -> TestDir {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let temp = TestDir(std::env::temp_dir().join(format!(
        "ge-modern-install-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir(&temp.0).unwrap();
    let source = temp.path().join("legacy");
    std::fs::create_dir_all(source.join("bin/64bit")).unwrap();
    std::fs::create_dir_all(source.join("data/modern-loader")).unwrap();
    #[cfg(target_os = "linux")]
    let (core, loader) = ("libgolden_core.so", "the_golden_eye.so");
    #[cfg(target_os = "windows")]
    let (core, loader) = ("golden_core.dll", "the_golden_eye.dll");
    std::fs::write(source.join("bin/64bit").join(core), b"committed-core").unwrap();
    std::fs::write(source.join("data/modern-loader").join(loader), b"modern-loader").unwrap();
    std::fs::write(source.join("data/templates"), b"templates").unwrap();
    temp
}

#[cfg(target_os = "linux")]
#[test]
fn publishes_complete_copy_once_and_preserves_both_installations() {
    let temp = seed();
    let source = temp.path().join("legacy");
    let destination = temp.path().join("modern/plugins/the_golden_eye");
    provision_linux(&source, &destination).unwrap();
    assert_eq!(std::fs::read(destination.join("bin/64bit/libgolden_core.so")).unwrap(), b"committed-core");
    assert_eq!(std::fs::read(destination.join("the_golden_eye.so")).unwrap(), b"modern-loader");
    assert_eq!(std::fs::read(destination.join("data/templates")).unwrap(), b"templates");
    std::fs::write(destination.join("user-file"), b"keep").unwrap();
    std::fs::write(source.join("bin/64bit/libgolden_core.so"), b"later-legacy-update").unwrap();
    provision_linux(&source, &destination).unwrap();
    assert_eq!(std::fs::read(destination.join("bin/64bit/libgolden_core.so")).unwrap(), b"committed-core");
    assert_eq!(std::fs::read(destination.join("user-file")).unwrap(), b"keep");
    assert!(source.join("data/templates").is_file());
}

#[cfg(target_os = "linux")]
#[test]
fn failures_leave_no_partial_installation_and_can_be_retried() {
    use std::os::unix::fs::symlink;
    let temp = seed();
    let source = temp.path().join("legacy");
    let destination = temp.path().join("modern/plugins/the_golden_eye");
    symlink(temp.path(), source.join("data/bad-link")).unwrap();
    assert!(provision_linux(&source, &destination).is_err());
    assert!(!destination.exists());
    assert_eq!(std::fs::read_dir(destination.parent().unwrap()).unwrap().count(), 0);
    std::fs::remove_file(source.join("data/bad-link")).unwrap();
    provision_linux(&source, &destination).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn refuses_unknown_destinations_and_symbolic_link_ancestors() {
    use std::os::unix::fs::symlink;
    let temp = seed();
    let source = temp.path().join("legacy");
    let destination = temp.path().join("modern");
    std::fs::create_dir(&destination).unwrap();
    assert!(provision_linux(&source, &destination).is_err());
    assert!(destination.is_dir());
    let link = temp.path().join("link");
    symlink(&destination, &link).unwrap();
    assert!(provision_linux(&source, &link.join("plugin")).is_err());
    assert!(!destination.join("plugin").exists());
}

#[cfg(target_os = "windows")]
#[test]
fn publishes_loader_once_without_replacing_any_other_file() {
    let temp = seed();
    let source = temp.path().join("legacy");
    provision_windows(&source).unwrap();
    assert_eq!(std::fs::read(source.join("the_golden_eye.dll")).unwrap(), b"modern-loader");
    std::fs::write(source.join("the_golden_eye.dll"), b"existing-loader").unwrap();
    provision_windows(&source).unwrap();
    assert_eq!(std::fs::read(source.join("the_golden_eye.dll")).unwrap(), b"existing-loader");
    assert_eq!(std::fs::read(source.join("bin/64bit/golden_core.dll")).unwrap(), b"committed-core");
}
