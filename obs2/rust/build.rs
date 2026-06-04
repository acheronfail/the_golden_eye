use std::env;
use std::path::PathBuf;

fn main() {
    // The HTTP server embeds the frontend bundle via include_str!(env!("BROWSER_BUNDLE")), rebuild when it changes.
    println!("cargo:rerun-if-env-changed=BROWSER_BUNDLE");

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
