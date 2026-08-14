//! Shared helper for the M3 pilot plugin tests: compiles the standalone
//! `plugins-src/` workspace (real DLL, real `LoadLibraryExW` path) once per
//! test process and stages the Prefetch plugin DLL into an isolated temp
//! directory. Mirrors the fixture-workspace pattern in
//! `plugin_fixture_util.rs`; `plugins-src/` is NOT a member of the
//! repository workspace and builds into `target/plugins-src/`.
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Stage the built Prefetch plugin DLL into a fresh temp directory. Keep
/// the returned `TempDir` alive for as long as the plugin is loaded.
pub fn stage_prefetch_plugin() -> tempfile::TempDir {
    let built = prefetch_plugin_dir();
    let temp = tempfile::tempdir().expect("create temp plugin dir");
    let file = "meow_plugin_prefetch.dll";
    std::fs::copy(built.join(file), temp.path().join(file))
        .unwrap_or_else(|error| panic!("stage prefetch plugin DLL: {error}"));
    temp
}

fn prefetch_plugin_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
    DIR.get_or_init(build_prefetch_plugin)
}

fn build_prefetch_plugin() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = manifest_dir.join("../../plugins-src/Cargo.toml");
    let target = manifest_dir.join("../../target/plugins-src");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = std::process::Command::new(cargo)
        .arg("build")
        .arg("--manifest-path")
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .expect("spawn cargo build for plugins-src");
    assert!(status.success(), "plugins-src build failed");
    target.join("debug")
}
