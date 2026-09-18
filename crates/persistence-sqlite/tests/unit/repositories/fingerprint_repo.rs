use super::*;
use crate::{open_in_memory, runner};
use domain::{
    CaseId, DataSource, DataSourceId, DataSourceKind, ForensicFingerprint, ForensicMetadata,
    ForensicObjectType,
};
use std::path::PathBuf;

fn fingerprint(object_id: &str) -> ForensicFingerprint {
    ForensicFingerprint::new(
        ForensicMetadata {
            object_id: object_id.to_string(),
            case_id: Some("case-1".to_string()),
            source_id: Some("source-1".to_string()),
            object_type: ForensicObjectType::FileEntry,
            parent_object_id: None,
            source_locator: Some("root/evidence.bin".to_string()),
            byte_length: Some(12),
            parser_id: Some("ntfs".to_string()),
            parser_version: Some("1".to_string()),
        },
        Some(&"A".repeat(64)),
    )
}

#[test]
fn upsert_round_trips_and_replaces_same_object() {
    let conn = open_in_memory().unwrap();
    runner::run_source_all(&conn).unwrap();
    let repo = ForensicFingerprintRepo::new(&conn);
    let first = fingerprint("file-1");
    repo.upsert(&first).unwrap();

    let mut changed = fingerprint("file-1");
    changed.source_locator = Some("root/renamed.bin".to_string());
    changed.metadata_sha256 = changed.metadata().metadata_sha256();
    repo.upsert(&changed).unwrap();

    let stored = repo
        .find(ForensicObjectType::FileEntry, Some("source-1"), "file-1")
        .unwrap()
        .unwrap();
    assert_eq!(stored.source_locator.as_deref(), Some("root/renamed.bin"));
    assert_eq!(repo.list_by_source("source-1").unwrap().len(), 1);
}

#[test]
fn invalid_content_digest_is_rejected_before_persistence() {
    let conn = open_in_memory().unwrap();
    runner::run_source_all(&conn).unwrap();
    let repo = ForensicFingerprintRepo::new(&conn);
    let mut value = fingerprint("file-2");
    value.content_sha256 = Some("not-a-sha256".to_string());
    assert!(repo.upsert(&value).is_err());
}

#[test]
fn data_source_hash_completion_updates_the_registered_fingerprint() {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at)
         VALUES ('case-1', 'Test case', datetime('now'), datetime('now'))",
        [],
    )
    .unwrap();
    let source = DataSource {
        id: DataSourceId("source-1".to_string()),
        name: "image.E01".to_string(),
        kind: DataSourceKind::E01,
        source_path: PathBuf::from("image.E01"),
        imported_at: chrono::Utc::now(),
        provenance: Default::default(),
    };
    crate::repositories::datasource_repo::DataSourceRepo::new(&conn)
        .insert(&CaseId("case-1".to_string()), &source)
        .unwrap();

    crate::repositories::datasource_repo::DataSourceRepo::new(&conn)
        .update_source_hash(
            &source.id,
            Some(&"A".repeat(64)),
            domain::DataSourceHashStatus::Hashed,
        )
        .unwrap();

    let stored = ForensicFingerprintRepo::new(&conn)
        .find(ForensicObjectType::DataSource, Some("source-1"), "source-1")
        .unwrap()
        .unwrap();
    assert_eq!(stored.content_sha256, Some("a".repeat(64)));
}
