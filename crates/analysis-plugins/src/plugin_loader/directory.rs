//! Plugin directory discovery (design doc §5.1).
//!
//! The host looks for parser plugin DLLs next to the running executable under
//! `plugins/<evidence-platform>/`. A missing directory is an empty plugin set,
//! never an error.

use std::path::{Path, PathBuf};

/// Directory name holding all evidence-platform plugin subdirectories.
pub const PLUGINS_DIR_NAME: &str = "plugins";

/// Evidence-platform subdirectories enumerated by the host. Both are scanned
/// on the Windows host; each plugin declares its own evidence platform.
pub const EVIDENCE_PLATFORM_SUBDIRS: [&str; 2] = ["windows", "linux"];

/// Default plugin root: `<current_exe_dir>/plugins`.
///
/// Returns `None` when the executable path cannot be determined; callers treat
/// that as an empty plugin set.
pub fn default_plugins_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(PLUGINS_DIR_NAME))
}

/// Existing evidence-platform subdirectories under `root`, in a deterministic
/// order. Missing subdirectories are skipped silently.
pub fn plugin_search_dirs(root: &Path) -> Vec<PathBuf> {
    EVIDENCE_PLATFORM_SUBDIRS
        .iter()
        .map(|subdir| root.join(subdir))
        .filter(|dir| dir.is_dir())
        .collect()
}
