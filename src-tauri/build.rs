fn main() {
    tauri_build::build();

    // libarchive2-sys's Linux build.rs omits -lb2 even when libb2 is detected by
    // CMake (the macOS arm includes it). RAR5 support in libarchive requires BLAKE2
    // from libb2, so we emit the link directive ourselves.
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "linux" {
        println!("cargo:rustc-link-lib=b2");
    }
}
