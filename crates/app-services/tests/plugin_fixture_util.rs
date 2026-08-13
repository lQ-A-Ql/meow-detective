//! Shared helper for plugin_loader integration tests: compiles the fixture
//! plugin workspace (real DLLs, real `LoadLibraryExW` path) once per test
//! process and stages selected DLLs into isolated temp directories.
//!
//! The fixture workspace lives at `tests/fixtures/plugin-fixtures/`, is NOT a
//! member of the repository workspace, and is built into
//! `target/plugin-fixtures/` so repeated test runs reuse the cargo cache.
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Compile (or reuse) the fixture plugin workspace and return the directory
/// containing the built DLLs.
pub fn fixture_plugins_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
    DIR.get_or_init(build_fixture_plugins)
}

/// Copy the built DLLs for the given fixture crate names (e.g. `"good"`,
/// `"good_dup"`) into a fresh temp directory. Keep the returned `TempDir`
/// alive for as long as the plugins are loaded.
pub fn stage_plugins(names: &[&str]) -> tempfile::TempDir {
    let built = fixture_plugins_dir();
    let temp = tempfile::tempdir().expect("create temp plugin dir");
    for name in names {
        let file = format!("meow_fixture_{}.dll", name.replace('-', "_"));
        std::fs::copy(built.join(&file), temp.path().join(&file))
            .unwrap_or_else(|error| panic!("stage fixture DLL {file}: {error}"));
    }
    temp
}

fn build_fixture_plugins() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = manifest_dir.join("tests/fixtures/plugin-fixtures/Cargo.toml");
    let target = manifest_dir.join("../../target/plugin-fixtures");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = std::process::Command::new(cargo)
        .arg("build")
        .arg("--manifest-path")
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .expect("spawn cargo build for fixture plugins");
    assert!(status.success(), "fixture plugin build failed");
    target.join("debug")
}
