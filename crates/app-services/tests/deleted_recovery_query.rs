use app_services::deleted_recovery::list_deleted_recoveries;
use domain::{CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    deleted_recovery_repo::{
        DeletedRecoveryAggregate, DeletedRecoveryRecord, DeletedRecoveryRepo, RecoveryRangeRecord,
        RecoveryScanRecord,
    },
};
use transport::dto::{RecoveryAllocationStateDto, RecoveryCompletenessDto};

#[test]
fn lists_source_local_recovery_candidates_through_the_service_boundary() {
    let case_root = tempfile::TempDir::new().unwrap();
    let case_conn = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&case_conn).unwrap();
    case_conn
        .execute("INSERT INTO cases (id, name) VALUES ('case-1', 'Case')", [])
        .unwrap();

    let case_id = CaseId("case-1".to_string());
    let data_source_id = DataSourceId("source-1".to_string());
    let source = DataSource {
        id: data_source_id.clone(),
        name: "Linux image".to_string(),
        kind: DataSourceKind::E01,
        source_path: case_root.path().join("evidence.E01"),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&data_source_id.0, Some("linux"), None);
    storage.import_state = "ready".to_string();
    DataSourceRepo::new(&case_conn)
        .insert_with_storage(&case_id, &source, &storage)
        .unwrap();

    let source_conn = app_services::source_db::open_source_db(case_root.path(), &data_source_id)
        .expect("create source database");
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(&case_id, &source)
        .unwrap();
    DeletedRecoveryRepo::new(&source_conn)
        .replace_scan(&recovery_aggregate())
        .unwrap();
    drop(source_conn);

    let page = list_deleted_recoveries(
        &case_conn,
        case_root.path(),
        &case_id,
        &data_source_id,
        2,
        0,
        100,
    )
    .unwrap();

    assert_eq!(page.total, 1);
    assert_eq!(page.scan.candidate_count, 1);
    assert_eq!(page.recoveries[0].inode, "77");
    assert_eq!(
        page.recoveries[0].completeness,
        RecoveryCompletenessDto::MetadataOnly
    );
    assert_eq!(
        page.recoveries[0].allocation_state,
        RecoveryAllocationStateDto::Unverified
    );
    assert_eq!(page.recoveries[0].recoverable_bytes, 0);
}

fn recovery_aggregate() -> DeletedRecoveryAggregate {
    DeletedRecoveryAggregate {
        scan: RecoveryScanRecord {
            id: "scan-1".to_string(),
            data_source_id: "source-1".to_string(),
            partition_index: 2,
            filesystem_type: "ext4".to_string(),
            filesystem_uuid: None,
            parser_version: "ext4-jbd2-v1".to_string(),
            log_kind: "internal_journal".to_string(),
            snapshot_identity_sha256: "a".repeat(64),
            state: "partial".to_string(),
            transaction_count: 1,
            candidate_count: 1,
            warnings: Vec::new(),
            started_at: "2026-07-21T00:00:00Z".to_string(),
            completed_at: "2026-07-21T00:00:01Z".to_string(),
        },
        recoveries: vec![DeletedRecoveryRecord {
            id: "candidate-77".to_string(),
            inode: "77".to_string(),
            original_path: Some("$OrphanInode/77".to_string()),
            entry_type: Some("file".to_string()),
            mode: Some(0o100644),
            mft_sequence: None,
            deleted_at_unix: Some(1_700_000_000),
            declared_size: 4_096,
            recoverable_bytes: 0,
            completeness: "metadata_only".to_string(),
            recovery_method: "jbd2_inode_table_snapshot".to_string(),
            confidence: 0.72,
            allocation_state: "unverified".to_string(),
            transaction_id: Some("tx-7".to_string()),
            log_sequence: Some(7),
            log_cycle: None,
            content_sha256: None,
            warnings: vec!["Content extents were not allocation-verified".to_string()],
            ranges: vec![RecoveryRangeRecord {
                ordinal: 0,
                range_role: "metadata".to_string(),
                source_kind: "journal".to_string(),
                logical_offset: 0,
                source_offset: 8_192,
                physical_offset: None,
                length: 256,
                allocation_state: "unverified".to_string(),
                sha256: None,
            }],
        }],
        issues: Vec::new(),
    }
}
