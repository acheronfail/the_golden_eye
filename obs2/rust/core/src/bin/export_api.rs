use std::path::PathBuf;

fn main() {
    let output = std::env::args_os().nth(1).map(PathBuf::from).expect("usage: export-api <output.ts>");
    ge_rust::export_api_contract(&output);
}
