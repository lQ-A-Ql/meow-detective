use persistence_sqlite::{
    open_in_memory,
    repositories::deleted_recovery_repo::{
        DeletedRecoveryAggregate, DeletedRecoveryRecord, DeletedRecoveryRepo, RecoveryIssueRecord,
        RecoveryRangeRecord, RecoveryScanRecord,
    },
    runner,
};

fn source_connection() -> rusqlite::Connection {
    let connection = open_in_memory().unwrap();
    runner::run_source_all(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO data_sources (
                id, case_id, name, kind, source_path, imported_at
             ) VALUES ('source-1', 'case-1', 'source', 'e01', 'evidence.E01', '2026-07-21T00:00:00Z')",
            [],
        )
        .unwrap();
    connection
}

fn aggregate(scan_id: &str, inode: &str) -> DeletedRecoveryAggregate {
    DeletedRecoveryAggregate {
        scan: RecoveryScanRecord {
            id: scan_id.to_string(),
            data_source_id: "source-1".to_string(),
            partition_index: 2,
            filesystem_type: "ext4".to_string(),
            filesystem_uuid: Some("01234567-89ab-cdef-0123-456789abcdef".to_string()),
            parser_version: "ext4-jbd2-v1".to_string(),
            log_kind: "internal_journal".to_string(),
            snapshot_identity_sha256: "a".repeat(64),
            state: "partial".to_string(),
            transaction_count: 3,
            candidate_count: 1,
            warnings: vec!["A revoked metadata block was omitted".to_string()],
            started_at: "2026-07-21T00:00:00Z".to_string(),
            completed_at: "2026-07-21T00:00:01Z".to_string(),
        },
        recoveries: vec![DeletedRecoveryRecord {
            id: format!("candidate-{inode}"),
            inode: inode.to_string(),
            original_path: Some(format!("$OrphanInode/{inode}")),
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
                sha256: Some("b".repeat(64)),
            }],
        }],
        issues: vec![RecoveryIssueRecord {
            ordinal: 0,
            severity: "warning".to_string(),
            code: "jbd2.revoked_block".to_string(),
            message: "A revoked metadata block was omitted".to_string(),
            log_offset: Some(12_288),
            sequence: Some(7),
        }],
    }
}

#[test]
fn source_migration_creates_recovery_tables() {
    let connection = source_connection();
    for table in [
        "filesystem_recovery_scans",
        "deleted_file_recoveries",
        "deleted_file_recovery_ranges",
        "filesystem_recovery_issues",
    ] {
        let exists: bool = connection
            .query_row(
                "SELECT COUNT(*) = 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists, "missing table {table}");
    }
}

#[test]
fn replaces_partition_scan_atomically_and_round_trips_provenance() {
    let connection = source_connection();
    let repo = DeletedRecoveryRepo::new(&connection);
    repo.replace_scan(&aggregate("scan-1", "42")).unwrap();

    let stored = repo.list_by_partition("source-1", 2).unwrap().unwrap();
    assert_eq!(stored, aggregate("scan-1", "42"));

    repo.replace_scan(&aggregate("scan-2", "84")).unwrap();
    let replaced = repo.list_by_partition("source-1", 2).unwrap().unwrap();
    assert_eq!(replaced, aggregate("scan-2", "84"));
    let scan_count: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM filesystem_recovery_scans",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(scan_count, 1);
}

#[test]
fn pages_candidates_without_loading_unrequested_ranges() {
    let connection = source_connection();
    let repo = DeletedRecoveryRepo::new(&connection);
    let mut stored = aggregate("scan-page", "42");
    let mut second = stored.recoveries[0].clone();
    second.id = "candidate-84".to_string();
    second.inode = "84".to_string();
    second.original_path = Some("$OrphanInode/84".to_string());
    second.ranges[0].source_offset = 32_768;
    stored.recoveries.push(second);
    stored.scan.candidate_count = 2;
    repo.replace_scan(&stored).unwrap();

    let page = repo.list_page("source-1", 2, 1, 1).unwrap().unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.offset, 1);
    assert_eq!(page.recoveries.len(), 1);
    assert_eq!(page.recoveries[0].inode, "84");
    assert_eq!(page.recoveries[0].ranges[0].source_offset, 32_768);
}

#[test]
fn recovery_lookup_is_scoped_to_the_requested_data_source() {
    let connection = source_connection();
    let repo = DeletedRecoveryRepo::new(&connection);
    repo.replace_scan(&aggregate("scan-lookup", "42")).unwrap();

    let found = repo
        .find_recovery("source-1", "candidate-42")
        .unwrap()
        .unwrap();
    assert_eq!(found.0.data_source_id, "source-1");
    assert_eq!(found.1.inode, "42");

    assert!(repo
        .find_recovery("source-2", "candidate-42")
        .unwrap()
        .is_none());
}

#[test]
fn rejects_metadata_only_candidate_that_claims_content() {
    let connection = source_connection();
    let repo = DeletedRecoveryRepo::new(&connection);
    let mut invalid = aggregate("scan-invalid", "42");
    invalid.recoveries[0].recoverable_bytes = 512;
    invalid.recoveries[0].ranges.push(RecoveryRangeRecord {
        ordinal: 1,
        range_role: "content".to_string(),
        source_kind: "filesystem".to_string(),
        logical_offset: 0,
        source_offset: 16_384,
        physical_offset: Some(16_384),
        length: 512,
        allocation_state: "unverified".to_string(),
        sha256: None,
    });

    let error = repo.replace_scan(&invalid).unwrap_err().to_string();
    assert!(error.contains("metadata-only recovery cannot claim recovered content"));
}

#[test]
fn accepts_verified_partial_content_and_rejects_unverified_content_ranges() {
    let connection = source_connection();
    let repo = DeletedRecoveryRepo::new(&connection);
    let mut partial = aggregate("scan-partial", "42");
    let recovery = &mut partial.recoveries[0];
    recovery.declared_size = 1_024;
    recovery.recoverable_bytes = 512;
    recovery.completeness = "partial".to_string();
    recovery.allocation_state = "partially_overwritten".to_string();
    recovery.ranges.push(RecoveryRangeRecord {
        ordinal: 1,
        range_role: "content".to_string(),
        source_kind: "filesystem".to_string(),
        logical_offset: 0,
        source_offset: 16_384,
        physical_offset: None,
        length: 512,
        allocation_state: "free".to_string(),
        sha256: Some("c".repeat(64)),
    });
    repo.replace_scan(&partial).unwrap();

    partial.scan.id = "scan-unverified".to_string();
    partial.recoveries[0].id = "candidate-unverified".to_string();
    partial.recoveries[0].ranges[1].allocation_state = "unverified".to_string();
    let error = repo.replace_scan(&partial).unwrap_err().to_string();
    assert!(error.contains("content ranges must be verified free"));
}

#[test]
fn complete_content_requires_contiguous_ranges_and_a_complete_digest() {
    let connection = source_connection();
    let repo = DeletedRecoveryRepo::new(&connection);
    let mut complete = aggregate("scan-complete", "42");
    let recovery = &mut complete.recoveries[0];
    recovery.declared_size = 1_024;
    recovery.recoverable_bytes = 1_024;
    recovery.completeness = "complete".to_string();
    recovery.allocation_state = "free".to_string();
    recovery.content_sha256 = Some("d".repeat(64));
    recovery.ranges.extend([
        RecoveryRangeRecord {
            ordinal: 1,
            range_role: "content".to_string(),
            source_kind: "filesystem".to_string(),
            logical_offset: 0,
            source_offset: 16_384,
            physical_offset: None,
            length: 512,
            allocation_state: "free".to_string(),
            sha256: Some("e".repeat(64)),
        },
        RecoveryRangeRecord {
            ordinal: 2,
            range_role: "content".to_string(),
            source_kind: "filesystem".to_string(),
            logical_offset: 512,
            source_offset: 32_768,
            physical_offset: None,
            length: 512,
            allocation_state: "free".to_string(),
            sha256: Some("f".repeat(64)),
        },
    ]);
    repo.replace_scan(&complete).unwrap();

    complete.scan.id = "scan-no-content-digest".to_string();
    complete.recoveries[0].id = "candidate-no-content-digest".to_string();
    complete.recoveries[0].content_sha256 = None;
    let error = repo.replace_scan(&complete).unwrap_err().to_string();
    assert!(error.contains("complete-content SHA-256 digest"));
}

#[test]
fn source_deletion_cascades_recovery_records() {
    let connection = source_connection();
    DeletedRecoveryRepo::new(&connection)
        .replace_scan(&aggregate("scan-1", "42"))
        .unwrap();
    connection
        .execute("DELETE FROM data_sources WHERE id = 'source-1'", [])
        .unwrap();

    let scan_count: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM filesystem_recovery_scans",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(scan_count, 0);
}

#[test]
fn ntfs_recovery_round_trips_mft_sequence() {
    let connection = source_connection();
    let repo = DeletedRecoveryRepo::new(&connection);
    let mut ntfs = aggregate("scan-ntfs", "1024");
    ntfs.scan.filesystem_type = "ntfs".to_string();
    ntfs.scan.parser_version = "ntfs-mft-v1".to_string();
    ntfs.scan.log_kind = "internal_log".to_string();
    let recovery = &mut ntfs.recoveries[0];
    recovery.mft_sequence = Some(9);
    recovery.recovery_method = "ntfs_mft_metadata".to_string();

    repo.replace_scan(&ntfs).unwrap();
    let stored = repo
        .find_recovery("source-1", "candidate-1024")
        .unwrap()
        .unwrap();

    assert_eq!(stored.0.filesystem_type, "ntfs");
    assert_eq!(stored.1.mft_sequence, Some(9));
}
