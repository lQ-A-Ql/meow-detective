use app_services::{case_service, datasource_service, deleted_recovery, file_service};
use image_e01::E01Reader;
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    deleted_recovery_repo::DeletedRecoveryRepo,
};
use sha2::{Digest, Sha256};
use std::io::Read;
use tempfile::TempDir;

fn fixture_path() -> std::path::PathBuf {
    testing::fixtures::local_liuyang_e01_fixture().unwrap_or_else(|| {
        panic!("set FORENSICS_LIUYANG_E01_FIXTURE to run ignored NTFS deleted recovery tests")
    })
}

fn sha256_file(path: &std::path::Path) -> String {
    let mut file = std::fs::File::open(path).unwrap();
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    hex::encode(hasher.finalize())
}

fn register_windows_source(
    active: &app_services::active_case::ActiveCase,
    source_path: &std::path::Path,
) -> domain::DataSourceId {
    let source_id = domain::DataSourceId("liuyang-ntfs-recovery".to_string());
    let source = domain::DataSource {
        id: source_id.clone(),
        name: "Liu Yang NTFS recovery fixture".to_string(),
        kind: domain::DataSourceKind::E01,
        source_path: source_path.to_path_buf(),
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&source_id.0, Some("windows"), None);
    storage.import_state = "ready".to_string();
    active
        .with_conn(|conn| {
            DataSourceRepo::new(conn).insert_with_storage(&active.meta.id, &source, &storage)?;
            Ok(())
        })
        .unwrap();

    let source_conn = app_services::source_db::open_source_db(&active.case_root, &source_id)
        .expect("create source database");
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(&active.meta.id, &source)
        .unwrap();
    let mut reader = E01Reader::open(source_path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs_count = probe
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.kind,
                datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .count();
    assert!(
        ntfs_count > 0,
        "Liu Yang fixture must expose an NTFS candidate"
    );
    file_service::store_data_source_partitions(&source_conn, &source_id, &probe.partitions)
        .unwrap();
    drop(source_conn);
    source_id
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_ntfs_deleted_recovery_scans_reads_and_exports_verified_candidates() {
    let fixture_path = fixture_path();
    let temp = TempDir::new().unwrap();
    let active = case_service::create_case(&temp.path().join("cases"), "liuyang-recovery", None)
        .expect("create temporary case");
    let source_id = register_windows_source(&active, &fixture_path);

    let first_run = active
        .with_conn(|conn| {
            deleted_recovery::run_deleted_recovery(
                conn,
                &active.case_root,
                &active.meta.id,
                &source_id,
                None,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("real NTFS deleted recovery scan should complete");
    assert!(
        first_run.failures.is_empty(),
        "NTFS scan failures: {:?}",
        first_run.failures
    );
    assert!(
        !first_run.scans.is_empty(),
        "Liu Yang fixture should produce at least one NTFS recovery scan"
    );
    assert!(
        first_run
            .scans
            .iter()
            .all(|scan| scan.filesystem_type == "ntfs"),
        "Windows recovery must not route non-NTFS partitions: {:?}",
        first_run.scans
    );
    let first_candidates = active
        .with_conn(|conn| {
            let scan = first_run.scans.first().unwrap();
            deleted_recovery::list_deleted_recoveries(
                conn,
                &active.case_root,
                &active.meta.id,
                &source_id,
                scan.partition_index,
                0,
                100,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("list persisted NTFS recovery candidates");
    assert_eq!(
        first_candidates.scan.candidate_count, first_candidates.total,
        "scan candidate count must match the persisted page total"
    );
    assert!(
        first_candidates
            .recoveries
            .iter()
            .all(|candidate| candidate.mft_sequence.is_some()),
        "every NTFS candidate must persist its MFT sequence"
    );
    assert!(
        first_candidates.total > 0,
        "Liu Yang fixture should expose inactive MFT candidates"
    );

    let content_candidate = first_candidates
        .recoveries
        .iter()
        .find(|candidate| {
            candidate.entry_type.as_deref() == Some("file")
                && candidate.recoverable_bytes > 0
                && candidate.completeness != transport::dto::RecoveryCompletenessDto::MetadataOnly
        })
        .cloned()
        .expect("Liu Yang fixture should expose a content-capable deleted NTFS candidate");
    let preview = active
        .with_conn(|conn| {
            deleted_recovery::read_deleted_recovery_range(
                conn,
                &active.case_root,
                &active.meta.id,
                &source_id,
                &content_candidate.id,
                0,
                4096,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("verified NTFS recovery range should be readable");
    assert!(
        preview.bytes_read > 0,
        "verified preview should return bytes"
    );
    assert!(!preview.verified_range_ordinals.is_empty());

    let export_path = temp.path().join("exports").join("recovered.bin");
    let exported = active
        .with_conn(|conn| {
            deleted_recovery::export_deleted_recovery(
                conn,
                &active.case_root,
                &active.meta.id,
                &source_id,
                &content_candidate.id,
                &export_path,
                false,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("complete verified NTFS candidate should export");
    assert_eq!(exported.bytes_written, content_candidate.declared_size);
    assert_eq!(exported.sha256, sha256_file(&export_path));
    assert_eq!(
        Some(exported.sha256.clone()),
        content_candidate.content_sha256
    );

    let second_run = active
        .with_conn(|conn| {
            deleted_recovery::run_deleted_recovery(
                conn,
                &active.case_root,
                &active.meta.id,
                &source_id,
                Some(content_candidate.partition_index),
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("repeated NTFS recovery scan should complete");
    assert_eq!(second_run.failures.len(), 0);
    let repeated = active
        .with_conn(|conn| {
            deleted_recovery::list_deleted_recoveries(
                conn,
                &active.case_root,
                &active.meta.id,
                &source_id,
                content_candidate.partition_index,
                0,
                100,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("list repeated NTFS recovery scan");
    let repeated_candidate = repeated
        .recoveries
        .iter()
        .find(|candidate| candidate.id == content_candidate.id)
        .expect("repeated scan must retain stable candidate identity");
    assert_eq!(
        repeated_candidate.mft_sequence,
        content_candidate.mft_sequence
    );
    assert_eq!(
        repeated_candidate.content_sha256,
        content_candidate.content_sha256
    );

    let source_conn = app_services::source_db::open_source_db(&active.case_root, &source_id)
        .expect("open source database after recovery");
    let persisted_scan = DeletedRecoveryRepo::new(&source_conn)
        .list_by_partition(&source_id.0, content_candidate.partition_index)
        .unwrap()
        .expect("source database must retain the recovery scan");
    assert_eq!(
        persisted_scan.scan.snapshot_identity_sha256,
        second_run.scans.first().unwrap().snapshot_identity_sha256
    );
}
