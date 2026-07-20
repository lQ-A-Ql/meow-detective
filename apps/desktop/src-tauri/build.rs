fn main() {
    tauri_build::build();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Tauri links its generated resource only into application binaries. Reuse
        // that same resource for test harnesses so Common Controls v6 is available.
        let output_dir = std::path::PathBuf::from(
            std::env::var_os("OUT_DIR").expect("Cargo did not provide OUT_DIR"),
        );
        println!(
            "cargo:rustc-link-search=native={}",
            output_dir.to_string_lossy()
        );
    }
}
