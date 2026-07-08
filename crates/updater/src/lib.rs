//! Production update checker, downloader, and installer.
//!
//! Delegates to `tauri-plugin-updater` for the Tauri-side manifest check and
//! native installer invocation. This crate adds a programmatic API that can be
//! driven from service code without coupling to Tauri command-scope.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::Digest;
use tracing::{debug, info, warn};

const APP_CODE_NAME: &str = "Meow_Detective";

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
        .user_agent(format!("{APP_CODE_NAME}/{}", current_version))
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
        .user_agent(format!("{APP_CODE_NAME}/{}", current_version))
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
    use std::path::Path;

    // -----------------------------------------------------------------------
    // is_newer tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_newer() {
        // Candidate is strictly greater
        assert!(is_newer("2.0.0", "1.0.0"));
        assert!(is_newer("1.1.0", "1.0.0"));
        assert!(is_newer("1.0.1", "1.0.0"));
        // Equal versions
        assert!(!is_newer("1.0.0", "1.0.0"));
        // Candidate is older
        assert!(!is_newer("0.9.0", "1.0.0"));
        // Pre-release comparison is not supported by simple numeric parser;
        // a proper semver crate should be used for pre-release ordering.
        assert!(!is_newer("5.0.0-rc1", "5.0.0-rc1"));
        assert!(is_newer("5.0.0-rc2", "5.0.0-rc1"));
        // Single-segment versions
        assert!(is_newer("10", "9"));
        assert!(!is_newer("9", "10"));
    }

    #[test]
    fn test_manifest_version_comparison() {
        // Equal versions return false
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("0.0.0", "0.0.0"));
        assert!(!is_newer("5", "5"));
        // Patch bump is newer
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("1.0.10", "1.0.9"));
        assert!(is_newer("1.0.100", "1.0.99"));
        // Minor bump is newer
        assert!(is_newer("1.1.0", "1.0.0"));
        assert!(is_newer("1.10.0", "1.9.0"));
        // Major bump is newer
        assert!(is_newer("2.0.0", "1.0.0"));
        assert!(is_newer("10.0.0", "9.9.9"));
        // Older versions are not newer
        assert!(!is_newer("0.9.9", "1.0.0"));
        assert!(!is_newer("1.0.0", "2.0.0"));
        assert!(!is_newer("1.0.0", "1.1.0"));
        assert!(!is_newer("1.0.0", "1.0.1"));
        // Different segment counts
        assert!(is_newer("1.0.0.1", "1.0.0"));
        assert!(!is_newer("1", "1.0.0"));
        assert!(!is_newer("1.0", "1.0.0"));
        // Large version numbers
        assert!(is_newer("999.999.999", "1.0.0"));
        assert!(!is_newer("1.0.0", "999.999.999"));
    }

    // -----------------------------------------------------------------------
    // Parse / manifest tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_update_manifest_valid() {
        let json = serde_json::json!({
            "version": "2.0.0",
            "notes": "Release notes for v2.0.0",
            "published_at": "2025-06-20T00:00:00Z",
            "download_url": "https://releases.example.com/meow-detective-2.0.0.msi",
            "sha256": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
            "size_bytes": 15_000_000
        });
        let manifest: UpdateManifest = serde_json::from_value(json).unwrap();
        assert_eq!(manifest.version, "2.0.0");
        assert_eq!(manifest.notes.as_deref(), Some("Release notes for v2.0.0"));
        assert_eq!(
            manifest.published_at.as_deref(),
            Some("2025-06-20T00:00:00Z")
        );
        assert_eq!(
            manifest.download_url.as_deref(),
            Some("https://releases.example.com/meow-detective-2.0.0.msi")
        );
        assert_eq!(
            manifest.sha256.as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
        );
        assert_eq!(manifest.size_bytes, Some(15_000_000));
    }

    #[test]
    fn test_parse_update_manifest_valid_minimal() {
        // Only "version" is required; all other fields are Option and
        // should deserialize to None when absent.
        let json = serde_json::json!({"version": "1.0.0"});
        let manifest: UpdateManifest = serde_json::from_value(json).unwrap();
        assert_eq!(manifest.version, "1.0.0");
        assert!(manifest.notes.is_none());
        assert!(manifest.published_at.is_none());
        assert!(manifest.download_url.is_none());
        assert!(manifest.sha256.is_none());
        assert!(manifest.size_bytes.is_none());
    }

    #[test]
    fn test_parse_update_manifest_invalid_json() {
        let result = serde_json::from_str::<UpdateManifest>("this is not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_update_manifest_missing_fields() {
        // "version" is the only required (non-Option) field.
        // Omitting it must produce a deserialization error.
        let result = serde_json::from_str::<UpdateManifest>(
            r#"{"notes": "some release notes", "size_bytes": 123}"#,
        );
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Serialization round-trip test
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_manifest_serialization() {
        let manifest = UpdateManifest {
            version: "3.2.1".to_string(),
            notes: Some("Test release notes".to_string()),
            published_at: Some("2025-01-15T12:00:00Z".to_string()),
            download_url: Some("https://releases.example.com/installer.msi".to_string()),
            sha256: Some(
                "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string(),
            ),
            size_bytes: Some(99_999),
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let roundtrip: UpdateManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.version, manifest.version);
        assert_eq!(roundtrip.notes, manifest.notes);
        assert_eq!(roundtrip.published_at, manifest.published_at);
        assert_eq!(roundtrip.download_url, manifest.download_url);
        assert_eq!(roundtrip.sha256, manifest.sha256);
        assert_eq!(roundtrip.size_bytes, manifest.size_bytes);
    }

    #[test]
    fn test_update_manifest_serialization_optional_fields_none() {
        // None fields serialize as JSON null (no skip_serializing_if on this struct).
        let manifest = UpdateManifest {
            version: "4.5.6".to_string(),
            notes: None,
            published_at: None,
            download_url: None,
            sha256: None,
            size_bytes: None,
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["version"], "4.5.6");
        assert!(parsed["notes"].is_null());
        assert!(parsed["published_at"].is_null());
        assert!(parsed["download_url"].is_null());
        assert!(parsed["sha256"].is_null());
        assert!(parsed["size_bytes"].is_null());
    }

    // -----------------------------------------------------------------------
    // apply_update test
    // -----------------------------------------------------------------------

    #[test]
    fn test_apply_update_path_does_not_exist() {
        let path = Path::new("C:\\completely\\nonexistent\\path\\installer.msi");
        let result = apply_update(path);
        assert!(matches!(result, Err(UpdateError::InstallerNotFound(_))));
    }

    // -----------------------------------------------------------------------
    // Async tests (wiremock)
    // -----------------------------------------------------------------------

    /// Current version is much higher than the manifest → no update.
    #[tokio::test]
    async fn test_check_update_returns_none_when_current() {
        let mock_server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "version": "1.0.0",
                    "notes": "An old release"
                })),
            )
            .mount(&mock_server)
            .await;

        let result = check_for_update("99.0.0", &mock_server.uri()).await;
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
        assert!(result.unwrap().is_none());
    }

    /// Current version is lower than the manifest → update available.
    #[tokio::test]
    async fn test_check_update_returns_some_when_newer() {
        let mock_server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "version": "2.0.0",
                    "notes": "A new release is available!",
                    "download_url": "https://releases.example.com/installer.msi",
                    "size_bytes": 5_000_000,
                    "published_at": "2025-06-20T00:00:00Z"
                })),
            )
            .mount(&mock_server)
            .await;

        let result = check_for_update("0.1.0", &mock_server.uri()).await;
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
        let manifest = result.unwrap().unwrap();
        assert_eq!(manifest.version, "2.0.0");
        assert_eq!(
            manifest.notes.as_deref(),
            Some("A new release is available!")
        );
        assert_eq!(
            manifest.download_url.as_deref(),
            Some("https://releases.example.com/installer.msi")
        );
    }

    /// Server returns HTTP 500 → FetchError.
    #[tokio::test]
    async fn test_check_update_fetch_error() {
        let mock_server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let result = check_for_update("1.0.0", &mock_server.uri()).await;
        assert!(
            matches!(result, Err(UpdateError::FetchError(_))),
            "expected FetchError, got {:?}",
            result
        );
    }

    /// Download from a server that returns 404 → DownloadError.
    #[tokio::test]
    async fn test_download_update_invalid_url() {
        let mock_server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let result = download_update(&mock_server.uri(), None, "1.0.0").await;
        assert!(
            matches!(result, Err(UpdateError::DownloadError(_))),
            "expected DownloadError, got {:?}",
            result
        );
    }

    /// Downloaded bytes do not match the expected SHA-256 → HashMismatch.
    #[tokio::test]
    async fn test_download_update_hash_mismatch() {
        let mock_server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string("some installer binary payload"),
            )
            .mount(&mock_server)
            .await;

        let result = download_update(
            &mock_server.uri(),
            Some("0000000000000000000000000000000000000000000000000000000000000000"),
            "1.0.0",
        )
        .await;
        assert!(
            matches!(result, Err(UpdateError::HashMismatch { .. })),
            "expected HashMismatch, got {:?}",
            result
        );
    }
}
