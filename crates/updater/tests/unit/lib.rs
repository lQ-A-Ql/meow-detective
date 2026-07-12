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
            wiremock::ResponseTemplate::new(200).set_body_string("some installer binary payload"),
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
