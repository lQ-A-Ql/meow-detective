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
fn direct_ntfs_volume_requires_a_derived_boot_layout_and_correct_case_scope() {
    let mut bytes = vec![0; 128 * 512];
    bytes[3..11].copy_from_slice(b"NTFS    ");
    bytes[11..13].copy_from_slice(&512u16.to_le_bytes());
    bytes[13] = 8;
    bytes[40..48].copy_from_slice(&64u64.to_le_bytes());
    bytes[48..56].copy_from_slice(&2u64.to_le_bytes());
    bytes[510..512].copy_from_slice(&[0x55, 0xaa]);
    let (_tmp, connection, source_id, _) = setup_ready_source(&bytes);
    let error = crate::mount_service::prepare_emulation_disk_layout(
        &connection,
        &CaseId("case-emulation-prepare".into()),
        &source_id,
        false,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        MountServiceError::DirectVolumeRecoveryRequired
    ));
    assert!(matches!(
        error.category(),
        transport::ErrorCategory::Unsupported
    ));
    let error = crate::mount_service::prepare_emulation_disk_layout(
        &connection,
        &CaseId("foreign-case".into()),
        &source_id,
        false,
    )
    .unwrap_err();
    assert!(matches!(error, MountServiceError::NotFound(_)));
}

#[test]
fn disk_layout_admission_does_not_misclassify_an_ordinary_raw_disk() {
    let (_tmp, connection, source_id, _) = setup_ready_source(&[0; 1024]);
    crate::mount_service::prepare_emulation_disk_layout(
        &connection,
        &CaseId("case-emulation-prepare".into()),
        &source_id,
        false,
    )
    .unwrap();
}

#[test]
fn verified_direct_volume_parent_preserves_source_hash_and_records_generated_layout() {
    let mut bytes = vec![0; 128 * 512];
    bytes[3..11].copy_from_slice(b"NTFS    ");
    bytes[11..13].copy_from_slice(&512u16.to_le_bytes());
    bytes[13] = 8;
    bytes[28..32].copy_from_slice(&649216u32.to_le_bytes());
    bytes[40..48].copy_from_slice(&64u64.to_le_bytes());
    bytes[48..56].copy_from_slice(&2u64.to_le_bytes());
    bytes[510..512].copy_from_slice(&[0x55, 0xaa]);
    let backup = bytes[..512].to_vec();
    bytes[64 * 512..65 * 512].copy_from_slice(&backup);
    let (_tmp, connection, source_id, digest) = setup_ready_source(&bytes);
    let parent = crate::mount_service::prepare_emulation_parent(
        &connection,
        &CaseId("case-emulation-prepare".into()),
        &source_id,
        true,
    )
    .unwrap();
    assert_eq!(hex::encode(parent.identity.sha256()), digest);
    assert_eq!(parent.provider.len(), 1024 * 1024 + 65 * 512);
    let provenance = parent.derived_ntfs.unwrap();
    assert_eq!(provenance.source_media_bytes, bytes.len() as u64);
    assert_eq!(provenance.source_volume_offset, 0);
    assert_eq!(provenance.source_volume_bytes, 65 * 512);
    assert_eq!(provenance.guest_partition_offset, 1024 * 1024);
    assert!(provenance.boot_repair_required);
    let mut mbr = [0; 512];
    parent.provider.read_exact_at(0, &mut mbr).unwrap();
    assert_eq!(mbr[450], 7);
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
