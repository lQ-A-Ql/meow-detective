use persistence_fjall::{
    DatabaseConfig, DeletedRecoveryAggregate, DeletedRecoveryHashAlgorithm, DeletedRecoveryRecord,
    DeletedRecoveryRecordStore, PersistMode, RecoveryIssueRecord, RecoveryRangeRecord,
    RecoveryScanRecord, ScanDirection,
};

fn aggregate(source: &str) -> DeletedRecoveryAggregate {
    DeletedRecoveryAggregate {
        scan: RecoveryScanRecord {
            id: "scan-1".to_owned(),
            data_source_id: source.to_owned(),
            partition_index: 0,
            filesystem_type: "ntfs".to_owned(),
            filesystem_uuid: Some("uuid".to_owned()),
            parser_version: "v1".to_owned(),
            log_kind: "journal".to_owned(),
            snapshot_identity_sha256: "a".repeat(64),
            state: "complete".to_owned(),
            transaction_count: 1,
            candidate_count: 1,
            warnings: Vec::new(),
            started_at: "2026-11-02T00:00:00Z".to_owned(),
            completed_at: "2026-11-02T00:00:01Z".to_owned(),
        },
        recoveries: vec![DeletedRecoveryRecord {
            id: "recovery-1".to_owned(),
            scan_id: "scan-1".to_owned(),
            inode: "42".to_owned(),
            original_path: Some("/deleted.txt".to_owned()),
            entry_type: Some("file".to_owned()),
            mode: Some(0o644),
            mft_sequence: Some(1),
            deleted_at_unix: Some(1),
            declared_size: 10,
            recoverable_bytes: 10,
            completeness: "complete".to_owned(),
            recovery_method: "carve".to_owned(),
            confidence: 0.9,
            allocation_state: "deleted".to_owned(),
            transaction_id: None,
            log_sequence: None,
            log_cycle: None,
            content_md5: Some("b".repeat(32)),
            content_sha1: None,
            content_sha256: Some("c".repeat(64)),
            warnings: Vec::new(),
            ranges: vec![RecoveryRangeRecord {
                ordinal: 0,
                range_role: "content".to_owned(),
                source_kind: "mft".to_owned(),
                logical_offset: 0,
                source_offset: 0,
                physical_offset: None,
                length: 10,
                allocation_state: "allocated".to_owned(),
                sha256: Some("d".repeat(64)),
            }],
        }],
        issues: vec![RecoveryIssueRecord {
            ordinal: 0,
            severity: "warning".to_owned(),
            code: "partial".to_owned(),
            message: "partial".to_owned(),
            log_offset: None,
            sequence: None,
        }],
    }
}

#[test]
fn deleted_recovery_replaces_pages_hashes_and_deletes_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let store = DeletedRecoveryRecordStore::open(dir.path(), DatabaseConfig::default()).unwrap();
    store
        .replace_scan(&aggregate("source-a"), PersistMode::SyncData)
        .unwrap();
    let snapshot = store.read_snapshot();
    let page = store
        .list_page(&snapshot, "source-a", 0, None, ScanDirection::Forward, 10)
        .unwrap()
        .unwrap();
    assert_eq!(page.recoveries.len(), 1);
    assert_eq!(page.issues.len(), 1);
    let by_hash = store
        .list_by_hash(
            &snapshot,
            "source-a",
            DeletedRecoveryHashAlgorithm::Sha256,
            &"c".repeat(64),
            None,
            ScanDirection::Forward,
            10,
        )
        .unwrap();
    assert_eq!(by_hash.recoveries.len(), 1);
    assert!(store
        .delete_scan("source-a", 0, PersistMode::SyncData)
        .unwrap());
    let snapshot = store.read_snapshot();
    assert!(store
        .list_page(&snapshot, "source-a", 0, None, ScanDirection::Forward, 10)
        .unwrap()
        .is_none());
}
