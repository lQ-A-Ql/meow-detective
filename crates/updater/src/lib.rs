//! Production update checker, downloader, and installer.
//!
//! Delegates to `tauri-plugin-updater` for the Tauri-side manifest check and
//! native installer invocation. This crate adds a programmatic API that can be
//! driven from service code without coupling to Tauri command-scope.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

mod download;
mod manifest;
mod version;

pub use download::download_update;
pub use manifest::check_for_update;

/// A parsed update manifest returned by the remote update server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    /// New version string (e.g. "5.0.0").
    pub version: String,
    /// Human-readable release notes (markdown).
    pub notes: Option<String>,
    /// Publication date in ISO 8601 format.
    pub published_at: Option<String>,
    /// Direct download URL for the platform installer.
    pub download_url: Option<String>,
    /// Optional SHA-256 hex digest of the installer file.
    pub sha256: Option<String>,
    /// Size of the installer in bytes.
    pub size_bytes: Option<u64>,
}

/// Error types for update operations.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("no update available")]
    NoUpdateAvailable,

    #[error("tls / rustls initialization failed: {0}")]
    TlsError(String),

    #[error("failed to fetch update manifest: {0}")]
    FetchError(String),

    #[error("download failed: {0}")]
    DownloadError(String),

    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("installer not found at {0}")]
    InstallerNotFound(PathBuf),

    #[error("failed to launch installer: {0}")]
    InstallError(String),
}

/// Apply a downloaded update by launching the platform installer.
///
/// On Windows this expects a `.msi` or `.exe`; on macOS a `.dmg`; on Linux an
/// `.AppImage` or `.deb`.
pub fn apply_update(installer_path: &Path) -> Result<(), UpdateError> {
    if !installer_path.exists() {
        return Err(UpdateError::InstallerNotFound(installer_path.to_path_buf()));
    }

    info!(path = %installer_path.display(), "launching installer");

    #[cfg(target_os = "windows")]
    {
        let path_str = installer_path.to_string_lossy();
        let status = std::process::Command::new("cmd.exe")
            .args(["/C", "start", "/WAIT", &path_str])
            .spawn()
            .map_err(|e| UpdateError::InstallError(e.to_string()))?
            .wait()
            .map_err(|e| UpdateError::InstallError(e.to_string()))?;

        if !status.success() {
            warn!(exit_code = ?status.code(), "installer exited non-zero");
        }
        // The installer process is expected to replace the running binary,
        // so we may never return from this call on success.
    }

    #[cfg(not(target_os = "windows"))]
    {
        return Err(UpdateError::InstallError(
            "apply_update only implemented for Windows hosts".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
