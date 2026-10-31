use domain::{CaseId, CaseMeta, DataSourceId, DataSourceKind, DataSourcePlatform};
use persistence_sqlite::repositories::{case_repo::CaseRepo, datasource_repo::DataSourceRepo};
use tempfile::TempDir;
use transport::ServiceErrorCategory;

use super::{
    prepare_emulation_source, source_identity_sha256, MountServiceError, PreparedPhysicalImageKind,
};

#[test]
fn source_identity_is_stable_and_scoped_to_the_case_and_source() {
    let case = CaseId("case-emulation-prepare".into());
    let source = DataSourceId("source-1".into());
    let identity = source_identity_sha256(&case, &source, PreparedPhysicalImageKind::Raw);

    assert_eq!(
        identity,
        source_identity_sha256(&case, &source, PreparedPhysicalImageKind::Raw)
    );
    assert_ne!(
        identity,
        source_identity_sha256(
            &CaseId("other-case".into()),
            &source,
            PreparedPhysicalImageKind::Raw
        )
    );
    assert_ne!(
        identity,
        source_identity_sha256(&case, &source, PreparedPhysicalImageKind::E01)
    );
}

#[test]
fn prepare_emulation_source_accepts_a_ready_import_without_a_persisted_digest() {
    let (_tmp, connection, source_id) = setup_ready_source(&[0u8; 512]);

    let prepared = prepare_emulation_source(
        &connection,
        &CaseId("case-emulation-prepare".into()),
        &source_id,
    )
    .unwrap();

    assert_eq!(
        prepared.source_identity_sha256,
        source_identity_sha256(
            &CaseId("case-emulation-prepare".into()),
            &source_id,
            PreparedPhysicalImageKind::Raw,
        )
    );
}

#[test]
fn public_e01_and_raw_fixtures_prepare_without_persisted_digests() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (name, kind, relative) in [
        (
            "trusted-e01",
            DataSourceKind::E01,
            "testdata/fixtures/public-small/e01/tiny.E01",
        ),
        (
            "trusted-raw",
            DataSourceKind::Raw,
            "testdata/fixtures/public-small/raw/tiny.raw",
        ),
    ] {
        let connection = persistence_sqlite::connection::open_in_memory().unwrap();
        persistence_sqlite::runner::run_all(&connection).unwrap();
        let case_id = CaseId(format!("case-{name}"));
        CaseRepo::new(&connection)
            .create(&CaseMeta {
                id: case_id.clone(),
                name: name.to_string(),
                number: None,
                examiner: None,
                notes: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
            .unwrap();
        let source = crate::datasource_service::attach_data_source(
            &connection,
            &case_id,
            name,
            &root.join(relative),
            kind,
            DataSourcePlatform::Windows,
        )
        .unwrap();
        DataSourceRepo::new(&connection)
            .update_import_state(&source.id, "ready", None)
            .unwrap();

        prepare_emulation_source(&connection, &case_id, &source.id).unwrap();
    }
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
    let (_tmp, connection, source_id) = setup_ready_source(&bytes);
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
    let (_tmp, connection, source_id) = setup_ready_source(&[0; 1024]);
    crate::mount_service::prepare_emulation_disk_layout(
        &connection,
        &CaseId("case-emulation-prepare".into()),
        &source_id,
        false,
    )
    .unwrap();
}

#[test]
fn verified_direct_volume_parent_preserves_source_identity_and_records_generated_layout() {
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
    let (_tmp, connection, source_id) = setup_ready_source(&bytes);
    let parent = crate::mount_service::prepare_emulation_parent(
        &connection,
        &CaseId("case-emulation-prepare".into()),
        &source_id,
        true,
    )
    .unwrap();
    assert_eq!(
        parent.identity.sha256(),
        &source_identity_sha256(
            &CaseId("case-emulation-prepare".into()),
            &source_id,
            PreparedPhysicalImageKind::Raw,
        )
    );
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
fn prepare_emulation_source_trusts_the_imported_source_after_same_length_change() {
    let original = vec![0x31u8; 512];
    let (tmp, connection, source_id) = setup_ready_source(&original);
    let mut tampered = original;
    tampered[0] ^= 0xff;
    std::fs::write(tmp.path().join("evidence.raw"), tampered).unwrap();

    let prepared = prepare_emulation_source(
        &connection,
        &CaseId("case-emulation-prepare".into()),
        &source_id,
    )
    .unwrap();
    assert_eq!(prepared.source_path, tmp.path().join("evidence.raw"));
}

#[test]
fn prepare_emulation_source_fails_closed_when_the_source_is_unreadable() {
    let (tmp, connection, source_id) = setup_ready_source(&[0u8; 512]);
    std::fs::remove_file(tmp.path().join("evidence.raw")).unwrap();

    let error = prepare_emulation_source(
        &connection,
        &CaseId("case-emulation-prepare".into()),
        &source_id,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        MountServiceError::SourceMetadata(_) | MountServiceError::Reader(_)
    ));
    assert!(matches!(error.category(), transport::ErrorCategory::Io));
}

fn setup_ready_source(bytes: &[u8]) -> (TempDir, rusqlite::Connection, DataSourceId) {
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
    (temporary, connection, source.id)
}
