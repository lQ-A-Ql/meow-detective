use super::*;

fn metadata() -> ForensicMetadata {
    ForensicMetadata {
        object_id: "file-1".to_string(),
        case_id: Some("case-1".to_string()),
        source_id: Some("source-1".to_string()),
        object_type: ForensicObjectType::FileEntry,
        parent_object_id: Some("dir-1".to_string()),
        source_locator: Some("Windows/System32/config/SYSTEM".to_string()),
        byte_length: Some(42),
        parser_id: Some("ntfs".to_string()),
        parser_version: Some("1".to_string()),
    }
}

#[test]
fn canonical_encoding_is_stable_and_versioned() {
    let value = metadata();
    assert_eq!(value.canonical_bytes(), metadata().canonical_bytes());
    assert_eq!(FMD_SCHEMA_VERSION, "fmd-v1");
    assert_eq!(value.metadata_sha256().len(), 64);
}

#[test]
fn metadata_changes_change_digest() {
    let first = metadata().metadata_sha256();
    let mut changed = metadata();
    changed.source_locator = Some("Windows/System32/config/SOFTWARE".to_string());
    assert_ne!(first, changed.metadata_sha256());
}

#[test]
fn content_hash_is_normalized_and_invalid_input_is_unavailable() {
    let uppercase = "A".repeat(64);
    let fingerprint = ForensicFingerprint::new(metadata(), Some(&uppercase));
    assert_eq!(fingerprint.content_sha256, Some("a".repeat(64)));

    let unavailable = ForensicFingerprint::new(metadata(), Some("not-a-sha256"));
    assert_eq!(unavailable.content_sha256, None);
    assert_eq!(
        normalize_content_sha256(Some(
            "  AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA  "
        )),
        Some("a".repeat(64))
    );
}

#[test]
fn object_type_round_trips_without_guessing() {
    for object_type in [
        ForensicObjectType::DataSource,
        ForensicObjectType::FileEntry,
        ForensicObjectType::Artifact,
        ForensicObjectType::Report,
    ] {
        assert_eq!(
            ForensicObjectType::parse(object_type.as_str()),
            Some(object_type)
        );
    }
    assert_eq!(ForensicObjectType::parse("unknown"), None);
}
