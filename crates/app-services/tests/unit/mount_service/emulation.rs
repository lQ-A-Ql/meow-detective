use std::sync::atomic::AtomicBool;

use domain::{
    CaseId, CaseMeta, DataSourceHashStatus, DataSourceId, DataSourceKind, DataSourcePlatform,
};
use persistence_sqlite::repositories::{case_repo::CaseRepo, datasource_repo::DataSourceRepo};
use tempfile::TempDir;
use transport::ServiceErrorCategory;

use super::{parse_source_sha256, prepare_emulation_source, MountServiceError};

#[test]
fn emulation_parent_binding_requires_an_exact_sha256() {
    assert_eq!(parse_source_sha256(&"ab".repeat(32)).unwrap(), [0xab; 32]);
    assert!(parse_source_sha256("data-source-id:source-1").is_err());
    assert!(parse_source_sha256(&"z".repeat(64)).is_err());
    assert!(parse_source_sha256(&"a".repeat(62)).is_err());
}

#[test]
fn prepare_emulation_source_verifies_the_parent_hash() {
    let (_tmp, connection, source_id, digest) = setup_ready_source(b"emulation evidence bytes");

    let prepared = prepare_emulation_source(&connection, &source_id).unwrap();

    assert_eq!(
        prepared.parent_sha256,
        parse_source_sha256(&digest).unwrap()
    );
}

#[test]
fn prepare_emulation_source_fails_closed_on_same_length_tampering() {
    let (tmp, connection, source_id, _) = setup_ready_source(b"emulation evidence bytes");
    let mut tampered = b"emulation evidence bytes".to_vec();
    tampered[0] ^= 0xff;
    std::fs::write(tmp.path().join("evidence.raw"), tampered).unwrap();

    let error = prepare_emulation_source(&connection, &source_id).unwrap_err();

    assert!(matches!(
        error,
        MountServiceError::SourceHashMismatch { .. }
    ));
    assert!(matches!(
        error.category(),
        transport::ErrorCategory::Security
    ));
}

#[test]
fn prepare_emulation_source_fails_closed_without_a_persisted_hash() {
    let (_tmp, connection, source_id, _) = setup_ready_source(b"emulation evidence bytes");
    DataSourceRepo::new(&connection)
        .update_source_hash(&source_id, None, DataSourceHashStatus::Pending)
        .unwrap();

    let error = prepare_emulation_source(&connection, &source_id).unwrap_err();

    assert!(matches!(error, MountServiceError::InvalidSourceFingerprint));
    assert!(matches!(
        error.category(),
        transport::ErrorCategory::Security
    ));
}

#[test]
fn prepare_emulation_source_fails_closed_when_the_source_is_unreadable() {
    let (tmp, connection, source_id, _) = setup_ready_source(b"emulation evidence bytes");
    connection
        .execute(
            "UPDATE data_sources SET evidence_size = NULL WHERE id = ?1",
            rusqlite::params![source_id.0],
        )
        .unwrap();
    std::fs::remove_file(tmp.path().join("evidence.raw")).unwrap();

    let error = prepare_emulation_source(&connection, &source_id).unwrap_err();

    assert!(matches!(error, MountServiceError::SourceHashVerify(_)));
    assert!(matches!(error.category(), transport::ErrorCategory::Io));
}

fn setup_ready_source(bytes: &[u8]) -> (TempDir, rusqlite::Connection, DataSourceId, String) {
    let connection = persistence_sqlite::connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&connection).unwrap();
    let case = CaseMeta {
        id: CaseId("case-emulation-prepare".to_string()),
        name: "Emulation Prepare".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    CaseRepo::new(&connection).create(&case).unwrap();
    let temporary = TempDir::new().unwrap();
    let source_path = temporary.path().join("evidence.raw");
    std::fs::write(&source_path, bytes).unwrap();
    let source = crate::datasource_service::attach_data_source(
        &connection,
        &case.id,
        "evidence",
        &source_path,
        DataSourceKind::Raw,
        DataSourcePlatform::Windows,
    )
    .unwrap();
    let repo = DataSourceRepo::new(&connection);
    repo.update_import_state(&source.id, "ready", None).unwrap();
    let digest = crate::hash_service::HashService::hash_evidence(
        &source_path,
        &DataSourceKind::Raw,
        &AtomicBool::new(false),
        &|_, _| {},
    )
    .unwrap()
    .digest;
    repo.update_source_hash(&source.id, Some(&digest), DataSourceHashStatus::Hashed)
        .unwrap();
    (temporary, connection, source.id, digest)
}
