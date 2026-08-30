use std::path::PathBuf;

// Strict linkers require definitions for OBS symbols retained by ge_rust.
#[path = "../obs_stub.rs"]
mod obs_stub;

fn main() {
    let output = std::env::args_os().nth(1).map(PathBuf::from).expect("usage: export-api <output.ts>");
    ge_rust::export_api_contract(&output);
}
