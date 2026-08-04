fn main() {
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to build Tauri application resources");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for(
            "windows-app-manifest.rc",
            ["forensics-desktop"],
            embed_resource::NONE,
        )
        .manifest_required()
        .expect("failed to compile the administrator application manifest");

        embed_resource::compile_for_tests("windows-test-manifest.rc", embed_resource::NONE)
            .manifest_required()
            .expect("failed to compile the non-elevated test manifest");

        // The library unit-test harness links the dedicated non-elevated
        // resource by name. The administrator manifest above is still linked
        // exclusively to the desktop binary by its full output path.
        let output_dir = std::path::PathBuf::from(
            std::env::var_os("OUT_DIR").expect("Cargo did not provide OUT_DIR"),
        );
        println!(
            "cargo:rustc-link-search=native={}",
            output_dir.to_string_lossy()
        );
    }
}
