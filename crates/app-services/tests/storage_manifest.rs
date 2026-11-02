use app_services::storage_manifest::{
    initialize_case, validate_case, StorageManifestError, STORAGE_FORMAT_VERSION,
    STORAGE_MANIFEST_FILE,
};
use tempfile::tempdir;

#[test]
fn initializes_and_validates_the_case_storage_layout() {
    let root = tempdir().unwrap();
    let manifest = initialize_case(root.path(), "case-1").unwrap();
    assert_eq!(manifest.format_version, STORAGE_FORMAT_VERSION);
    assert!(root.path().join("sources").is_dir());
    assert!(root.path().join("artifacts/objects").is_dir());
    assert!(root.path().join("fjall").is_dir());
    assert_eq!(validate_case(root.path(), "case-1").unwrap(), manifest);
}

#[test]
fn rejects_missing_wrong_version_and_wrong_case_manifests() {
    let root = tempdir().unwrap();
    assert!(matches!(
        validate_case(root.path(), "case-1"),
        Err(StorageManifestError::Missing(_))
    ));
    initialize_case(root.path(), "case-1").unwrap();
    let path = root.path().join(STORAGE_MANIFEST_FILE);
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["formatVersion"] = serde_json::json!(99);
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(
        validate_case(root.path(), "case-1"),
        Err(StorageManifestError::Unsupported(99))
    ));
    initialize_case(root.path(), "case-1").unwrap();
    value["formatVersion"] = serde_json::json!(STORAGE_FORMAT_VERSION);
    value["caseId"] = serde_json::json!("other-case");
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(
        validate_case(root.path(), "case-1"),
        Err(StorageManifestError::CaseIdentity(_))
    ));
}
