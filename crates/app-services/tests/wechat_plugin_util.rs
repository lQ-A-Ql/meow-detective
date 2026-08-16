//! Shared helper for the WeChat plugin regression: reuses the
//! `plugins-src/` workspace build (see `bt_panel_plugin_util.rs`) and stages
//! the WeChat plugin DLL into an isolated temp directory.
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Stage the built WeChat plugin DLL into a fresh temp directory. Keep
/// the returned `TempDir` alive for as long as the plugin is loaded.
pub fn stage_wechat_plugin() -> tempfile::TempDir {
    let built = wechat_plugin_dir();
    let temp = tempfile::tempdir().expect("create temp plugin dir");
    let file = "meow_plugin_wechat.dll";
    std::fs::copy(built.join(file), temp.path().join(file))
        .unwrap_or_else(|error| panic!("stage wechat plugin DLL: {error}"));
    temp
}

fn wechat_plugin_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
    DIR.get_or_init(build_plugins_workspace)
}

fn build_plugins_workspace() -> PathBuf {
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
