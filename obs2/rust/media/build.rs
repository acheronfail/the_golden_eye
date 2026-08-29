use std::env;

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    // Static FFmpeg references these Windows SDK libraries, but its Cargo
    // probe does not emit the complete set for standalone test binaries.
    for lib in
        ["bcrypt", "crypt32", "mfplat", "mfuuid", "ncrypt", "ntdll", "ole32", "secur32", "strmiids", "user32", "ws2_32"]
    {
        println!("cargo:rustc-link-lib={lib}");
    }
}
