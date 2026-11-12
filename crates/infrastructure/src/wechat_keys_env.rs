//! WeChat plugin key-recovery hand-off.
//!
//! The analysis extraction run points `MEOW_WECHAT_KEYS` at the keys file
//! recovered into the case workspace so the plugin's optional action channel
//! can decrypt images. Without a keys file nothing changes.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Environment variable consumed by the WeChat plugin.
pub const WECHAT_KEYS_ENV: &str = "MEOW_WECHAT_KEYS";

/// Case-workspace location of the recovered WeChat keys file.
pub fn keys_file_path(case_root: &Path) -> PathBuf {
    case_root
        .join("derived")
        .join("wechat-keys")
        .join("keys.json")
}

/// RAII guard pointing the plugin's key-injection channel at the recovered
/// keys file for the duration of one analysis extraction run.
///
/// Process-level env caveat: this is a single-user desktop application and
/// analysis extraction is admitted through a bounded scheduler (a single
/// extraction slot), so the set/restore window cannot race a concurrent
/// extraction.
pub struct WeChatKeysEnvGuard {
    previous: Option<OsString>,
}

impl WeChatKeysEnvGuard {
    /// Activates the hand-off when a recovered keys file exists.
    pub fn activate(case_root: &Path) -> Option<Self> {
        let path = keys_file_path(case_root);
        if !path.is_file() {
            return None;
        }
        let previous = std::env::var_os(WECHAT_KEYS_ENV);
        std::env::set_var(WECHAT_KEYS_ENV, &path);
        Some(Self { previous })
    }
}

impl Drop for WeChatKeysEnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(WECHAT_KEYS_ENV, value),
            None => std::env::remove_var(WECHAT_KEYS_ENV),
        }
    }
}
