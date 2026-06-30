use std::env;
use std::path::PathBuf;

fn main() {
    // The HTTP server embeds the frontend bundle via include_str!(env!("BROWSER_BUNDLE")), rebuild when it changes.
    println!("cargo:rerun-if-env-changed=BROWSER_BUNDLE");

    // Emitting any rerun-if-* above disables cargo's default "rerun on any file
    // change", which would otherwise pin this build script to BROWSER_BUNDLE and
    // let the cbindgen-generated ge_rust.h drift out of sync with the Rust FFI.
    // Re-run (and regenerate the header) whenever a source file changes so the C
    // side (core.c / plugin.c) always sees the current signatures.
    emit_rerun_for_sources("src");

    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let output_file = PathBuf::from(&crate_dir).join("..").join("ge_rust.h");

    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_language(cbindgen::Language::C)
        .with_include_guard("GE_RUST_H")
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
