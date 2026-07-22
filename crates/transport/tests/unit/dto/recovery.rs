use super::*;

fn recovery() -> DeletedFileRecoveryDto {
    DeletedFileRecoveryDto {
        id: "candidate-77".to_string(),
        data_source_id: "source-1".to_string(),
        partition_index: 2,
        filesystem_type: "ext4".to_string(),
        filesystem_uuid: Some("01234567-89ab-cdef-0123-456789abcdef".to_string()),
        inode: "77".to_string(),
        original_path: Some("$OrphanInode/77".to_string()),
        entry_type: Some("file".to_string()),
        mode: Some(0o100644),
        mft_sequence: None,
        deleted_at_unix: Some(1_700_000_000),
        declared_size: 4_096,
        recoverable_bytes: 0,
        completeness: RecoveryCompletenessDto::MetadataOnly,
        allocation_state: RecoveryAllocationStateDto::Unverified,
        recovery_method: "jbd2_inode_table_snapshot".to_string(),
        confidence: 0.72,
        transaction_id: Some("tx-7".to_string()),
        log_sequence: Some(7),
        log_cycle: None,
        content_sha256: None,
        provenance_ranges: vec![RecoveryProvenanceRangeDto {
            ordinal: 0,
            range_role: "metadata".to_string(),
            source_kind: "journal".to_string(),
            logical_offset: 0,
            source_offset: 8_192,
            physical_offset: None,
            length: 256,
            allocation_state: RecoveryAllocationStateDto::Unverified,
            sha256: Some("a".repeat(64)),
        }],
        warnings: vec!["Content extents were not allocation-verified".to_string()],
    }
}

#[test]
fn deleted_file_recovery_serializes_camel_case_and_forensic_states() {
    let value = serde_json::to_value(recovery()).unwrap();

    assert_eq!(value["dataSourceId"], "source-1");
    assert_eq!(value["partitionIndex"], 2);
    assert_eq!(value["completeness"], "metadata_only");
    assert_eq!(value["allocationState"], "unverified");
    assert_eq!(value["recoverableBytes"], 0);
    assert_eq!(value["provenanceRanges"][0]["sourceOffset"], 8_192);
    assert!(value["provenanceRanges"][0].get("physicalOffset").is_none());
    assert!(value.get("data_source_id").is_none());
    assert!(value.get("logCycle").is_none());
    assert!(value.get("contentSha256").is_none());
}

#[test]
fn ntfs_recovery_round_trips_mft_sequence_in_camel_case() {
    let mut ntfs = recovery();
    ntfs.filesystem_type = "ntfs".to_string();
    ntfs.inode = "1024".to_string();
    ntfs.mft_sequence = Some(9);

    let value = serde_json::to_value(&ntfs).unwrap();
    assert_eq!(value["mftSequence"], 9);
    assert!(value.get("mft_sequence").is_none());

    let decoded: DeletedFileRecoveryDto = serde_json::from_value(value).unwrap();
    assert_eq!(decoded, ntfs);
}

#[test]
fn recovery_scan_round_trips_without_inventing_content() {
    let scan = DeletedRecoveryScanDto {
        id: "scan-1".to_string(),
        data_source_id: "source-1".to_string(),
        partition_index: 2,
        filesystem_type: "ext4".to_string(),
        filesystem_uuid: None,
        parser_version: "ext4-jbd2-v1".to_string(),
        log_kind: "internal_journal".to_string(),
        snapshot_identity_sha256: "b".repeat(64),
        state: RecoveryScanStateDto::Partial,
        transaction_count: 3,
        candidate_count: 1,
        warnings: Vec::new(),
        started_at: "2026-07-21T00:00:00Z".to_string(),
        completed_at: "2026-07-21T00:00:01Z".to_string(),
        issues: vec![RecoveryIssueDto {
            ordinal: 0,
            severity: RecoveryIssueSeverityDto::Warning,
            code: "jbd2.revoked_block".to_string(),
            message: "A revoked block was omitted".to_string(),
            log_offset: Some(12_288),
            sequence: Some(7),
        }],
    };

    let encoded = serde_json::to_string(&scan).unwrap();
    let decoded: DeletedRecoveryScanDto = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, scan);

    let page = DeletedRecoveryPageDto {
        scan,
        recoveries: vec![recovery()],
        offset: 0,
        limit: 100,
        total: 1,
    };
    let page_value = serde_json::to_value(page).unwrap();
    assert_eq!(page_value["recoveries"][0]["completeness"], "metadata_only");
    assert_eq!(page_value["total"], 1);
}

#[test]
fn recovery_content_and_export_dtos_preserve_integrity_metadata() {
    let range = DeletedRecoveryContentRangeDto {
        recovery_id: format!("recovery:{}", "a".repeat(64)),
        offset: 4,
        bytes_base64: "YWJjZA==".to_string(),
        bytes_read: 4,
        declared_size: 8,
        eof: true,
        verified_range_ordinals: vec![1, 2],
    };
    let value = serde_json::to_value(&range).unwrap();
    assert_eq!(value["recoveryId"], range.recovery_id);
    assert_eq!(value["verifiedRangeOrdinals"], serde_json::json!([1, 2]));

    let export = DeletedRecoveryExportDto {
        recovery_id: range.recovery_id,
        bytes_written: 8,
        sha256: "b".repeat(64),
    };
    let value = serde_json::to_value(export).unwrap();
    assert_eq!(value["bytesWritten"], 8);
    assert_eq!(value["sha256"], "b".repeat(64));
}
