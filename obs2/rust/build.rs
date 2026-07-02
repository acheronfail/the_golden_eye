use std::env;
use std::path::PathBuf;

fn main() {
    // The HTTP server embeds the frontend bundle via include_str!(env!("BROWSER_BUNDLE")), rebuild when it changes.
    println!("cargo:rerun-if-env-changed=BROWSER_BUNDLE");
    println!("cargo:rerun-if-env-changed=GE_PLUGIN_VERSION");
    println!("cargo:rerun-if-env-changed=GE_BROWSER_DEV_URL");

    println!("cargo:rerun-if-env-changed=OPENCV_INCLUDE_PATHS");
    println!("cargo:rerun-if-env-changed=OPENCV_LINK_PATHS");
    println!("cargo:rerun-if-env-changed=OPENCV_LINK_LIBS");
    println!("cargo:rerun-if-env-changed=OPENCV_DISABLE_PROBES");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

    // Emitting any rerun-if-* above disables cargo's default "rerun on any file
    // change", which would otherwise pin this build script to BROWSER_BUNDLE and
    // let the cbindgen-generated ge_rust.h drift out of sync with the Rust FFI.
    // Re-run (and regenerate the header) whenever a source file changes so the C
    // side (core.c / plugin.c) always sees the current signatures.
    emit_rerun_for_sources("src");

    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let obs2_dir = crate_dir.parent().expect("Rust crate should live under obs2/").to_path_buf();
    let output_file = obs2_dir.join("ge_rust.h");

    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_language(cbindgen::Language::C)
        .with_include_guard("GE_RUST_H")
        // Raw OBS imports are for Rust's use only. Exposing obs_queue_task here
        // gives C a duplicate declaration that conflicts with OBS's own header.
        .exclude_item("obs_queue_task")
        .exclude_item("obs_frontend_get_user_config")
        .exclude_item("config_get_string")
        .exclude_item("config_set_string")
        .exclude_item("config_save_safe")
        .exclude_item("ObsTaskType")
        .exclude_item("ObsTask")
        .generate()
        .expect("Unable to generate cbindgen bindings")
        .write_to_file(output_file);
}

/// Recursively emit `cargo:rerun-if-changed` for every file under `dir`, so the
/// header is regenerated on any source edit (a bare `rerun-if-changed=src` only
/// tracks the directory's own mtime, missing edits to files within it).
fn emit_rerun_for_sources(dir: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            emit_rerun_for_sources(&path.to_string_lossy());
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
