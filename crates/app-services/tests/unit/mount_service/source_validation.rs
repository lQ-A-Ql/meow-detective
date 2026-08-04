use std::io::Write;

use domain::{DataSourceId, DataSourceKind};

use super::{mount_source_binding, validate_source_identity, validate_source_kind};

#[test]
fn mount_binding_prefers_the_persisted_evidence_hash() {
    let data_source_id = DataSourceId("source-1".to_string());
    let hash = "a".repeat(64);
    assert_eq!(
        mount_source_binding(&data_source_id, Some(hash.clone())),
        hash
    );
}

#[test]
fn mount_binding_uses_the_source_id_without_hashing_the_evidence() {
    let data_source_id = DataSourceId("source-1".to_string());
    assert_eq!(
        mount_source_binding(&data_source_id, None),
        "data-source-id:source-1"
    );
    assert_eq!(
        mount_source_binding(&data_source_id, Some("  ".to_string())),
        "data-source-id:source-1"
    );
}

#[test]
fn mount_source_kind_is_limited_to_physical_image_readers() {
    assert!(validate_source_kind(&DataSourceKind::E01).is_ok());
    assert!(validate_source_kind(&DataSourceKind::Raw).is_ok());
    assert!(validate_source_kind(&DataSourceKind::LogicalDirectory).is_err());
    assert!(validate_source_kind(&DataSourceKind::CephRbd).is_err());
    assert!(validate_source_kind(&DataSourceKind::CephFs).is_err());
}

#[test]
fn mount_source_identity_rejects_a_changed_evidence_size() {
    let mut source = tempfile::NamedTempFile::new().unwrap();
    source.write_all(b"evidence").unwrap();
    assert!(validate_source_identity(source.path(), Some(8)).is_ok());
    assert!(validate_source_identity(source.path(), None).is_ok());
    assert!(validate_source_identity(source.path(), Some(7)).is_err());
}
