//! Production update checker, downloader, and installer.
//!
//! Delegates to `tauri-plugin-updater` for the Tauri-side manifest check and
//! native installer invocation. This crate adds a programmatic API that can be
//! driven from service code without coupling to Tauri command-scope.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::Digest;
use tracing::{debug, info, warn};

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

/// Check for available update by querying the remote update server.
///
/// Returns `Ok(None)` when the current version is already the latest.
/// Returns `Ok(Some(manifest))` when a newer version is available.
pub async fn check_for_update(
    current_version: &str,
    update_endpoint: &str,
) -> Result<Option<UpdateManifest>, UpdateError> {
    debug!(current_version, update_endpoint, "checking for update");

    let client = reqwest::Client::builder()
        .user_agent(format!("forensics-workbench/{}", current_version))
        .build()
        .map_err(|e| UpdateError::TlsError(e.to_string()))?;

    let response = client
        .get(update_endpoint)
        .send()
        .await
        .map_err(|e| UpdateError::FetchError(e.to_string()))?;

    if !response.status().is_success() {
        return Err(UpdateError::FetchError(format!(
            "HTTP {}",
            response.status()
        )));
    }

    let manifest: UpdateManifest = response
        .json()
        .await
        .map_err(|e| UpdateError::FetchError(e.to_string()))?;

    // Compare versions using semver-like simple comparison.
    // In production, consider using the `semver` crate.
    if !is_newer(&manifest.version, current_version) {
        info!(
            latest = %manifest.version,
            current = %current_version,
            "already up to date"
        );
        return Ok(None);
    }

    info!(
        latest = %manifest.version,
        current = %current_version,
        "update available"
    );
    Ok(Some(manifest))
}

/// Download the update installer to a temporary file path.
///
/// If `expected_sha256` is provided, the downloaded file is verified against
/// it before returning the path.
pub async fn download_update(
    url: &str,
    expected_sha256: Option<&str>,
    current_version: &str,
) -> Result<PathBuf, UpdateError> {
    info!(url, "downloading update");

    let client = reqwest::Client::builder()
        .user_agent(format!("forensics-workbench/{}", current_version))
        .build()
        .map_err(|e| UpdateError::TlsError(e.to_string()))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| UpdateError::DownloadError(e.to_string()))?;

    if !response.status().is_success() {
        return Err(UpdateError::DownloadError(format!(
            "HTTP {}",
            response.status()
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpdateError::DownloadError(e.to_string()))?;

    // Optionally verify the hash before writing to disk.
    if let Some(expected) = expected_sha256 {
        let actual = hex::encode(sha2::Sha256::digest(&bytes));
        if actual != expected {
            return Err(UpdateError::HashMismatch {
                expected: expected.to_string(),
                actual,
            });
        }
        info!("sha256 checksum verified");
    }

    // Determine the installer extension from the URL.
    let ext = url
        .split('?')
        .next()
        .and_then(|path| Path::new(path).extension())
        .and_then(|e| e.to_str())
        .unwrap_or("tmp");

    let suffix = format!(".{}", ext);
    let temp_file = tempfile::Builder::new()
        .suffix(&suffix)
        .tempfile()
        .map_err(|e| UpdateError::DownloadError(e.to_string()))?;
    let path = temp_file.path().to_path_buf();

    std::fs::write(&path, &bytes).map_err(|e| UpdateError::DownloadError(e.to_string()))?;

    // Persist the tempfile so it is not deleted immediately.
    // `keep()` returns `(File, PathBuf)` — we only need the path.
    let (_file, persisted_path) = temp_file
        .keep()
        .map_err(|e| UpdateError::DownloadError(e.to_string()))?;
    info!(path = %persisted_path.display(), "update downloaded");

    Ok(persisted_path)
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Simple semver-like comparison: returns `true` if `candidate` is strictly
/// greater than `reference`.
fn is_newer(candidate: &str, reference: &str) -> bool {
    fn parse_parts(v: &str) -> Vec<u64> {
        v.split(|c: char| !c.is_ascii_digit())
            .filter_map(|s| s.parse::<u64>().ok())
            .collect()
    }

    let a = parse_parts(candidate);
    let b = parse_parts(reference);
    let len = a.len().max(b.len());
    for i in 0..len {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        match av.cmp(&bv) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer() {
        assert!(is_newer("2.0.0", "1.0.0"));
        assert!(is_newer("1.1.0", "1.0.0"));
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.0", "1.0.0"));
        // Pre-release comparison is not supported by simple numeric parser;
        // a proper semver crate should be used for pre-release ordering.
        assert!(!is_newer("5.0.0-rc1", "5.0.0-rc1"));
        assert!(is_newer("5.0.0-rc2", "5.0.0-rc1"));
        assert!(is_newer("10", "9"));
    }
}
