use super::*;

#[test]
fn maps_exact_journal_inode_provenance_instead_of_the_whole_payload_block() {
    let mut journal = vec![0u8; 1_024];
    journal[300..556].fill(0x5a);
    let candidate = fs_ext4::journal::DeletedInodeCandidate {
        inode: 77,
        kind: fs_ext4::journal::DeletedInodeKind::RegularFile,
        mode: 0o100644,
        declared_size: 4_096,
        deletion_time: 1_700_000_000,
        transaction_sequence: 9,
        descriptor_journal_block: 1,
        payload_journal_block: 2,
        inode_offset_within_payload: 44,
        journal_source_offset: 300,
        journal_source_length: 256,
        inode_table_group: 0,
        inode_table_block: 12,
        tag_marked_deleted: false,
        replay_revoked: false,
        journal_checksum_verified: true,
        inode_checksum_verified: false,
        completeness: fs_ext4::journal::RecoveryCompleteness::MetadataOnly,
        recoverable_bytes: 0,
        content_mapping: fs_ext4::journal::DeletedContentMapping {
            state: fs_ext4::journal::DeletedContentMappingState::Unsupported,
            inode_allocation_state: fs_ext4::journal::RecoveryAllocationState::Unverified,
            data_allocation_state: fs_ext4::journal::RecoveryAllocationState::Unverified,
            ranges: Vec::new(),
            recoverable_bytes: 0,
            content_md5: None,
            content_sha1: None,
            content_sha256: None,
            issue: None,
        },
        confidence: 0.9,
        recovery_method: "jbd2_inode_table_snapshot".to_string(),
    };

    let recovery = candidate_record("source-1", 2, &"a".repeat(64), &journal, candidate)
        .expect("map candidate");

    assert_eq!(recovery.ranges.len(), 1);
    assert_eq!(recovery.ranges[0].source_offset, 300);
    assert_eq!(recovery.ranges[0].length, 256);
    assert_eq!(
        recovery.ranges[0].sha256.as_deref(),
        Some(sha256_hex(&journal[300..556]).as_str())
    );
    assert_eq!(recovery.recoverable_bytes, 0);
    assert_eq!(recovery.completeness, "metadata_only");
}

#[test]
fn persists_only_verified_free_content_and_emits_an_honest_complete_claim() {
    let journal = vec![0x5a; 1_024];
    let content = [1u8, 2, 3, 4];
    let candidate = candidate_with_content(
        4,
        fs_ext4::journal::RecoveryAllocationState::Free,
        vec![fs_ext4::journal::DeletedContentRange {
            logical_offset: 0,
            filesystem_block: Some(30),
            filesystem_source_offset: Some(30_720),
            length: 4,
            kind: fs_ext4::journal::DeletedContentRangeKind::RecoverableData,
            allocation_state: fs_ext4::journal::RecoveryAllocationState::Free,
            sha256: Some(sha256_hex(&content)),
        }],
        Some(sha256_hex(&content)),
    );

    let recovery = candidate_record("source-1", 2, &"a".repeat(64), &journal, candidate)
        .expect("map complete candidate");

    assert_eq!(recovery.completeness, "complete");
    assert_eq!(recovery.allocation_state, "free");
    assert_eq!(recovery.recoverable_bytes, 4);
    assert_eq!(recovery.content_md5, Some("a".repeat(32)));
    assert_eq!(recovery.content_sha1, Some("b".repeat(40)));
    assert_eq!(recovery.content_sha256, Some(sha256_hex(&content)));
    assert_eq!(recovery.ranges.len(), 2);
    assert_eq!(recovery.ranges[1].range_role, "content");
    assert_eq!(recovery.ranges[1].allocation_state, "free");
    assert!(!recovery
        .warnings
        .iter()
        .any(|warning| warning.contains("only deleted inode metadata")));
}

#[test]
fn mixed_allocation_persists_only_free_ranges_as_partial_content() {
    let journal = vec![0x5a; 1_024];
    let candidate = candidate_with_content(
        8,
        fs_ext4::journal::RecoveryAllocationState::Mixed,
        vec![
            fs_ext4::journal::DeletedContentRange {
                logical_offset: 0,
                filesystem_block: Some(30),
                filesystem_source_offset: Some(30_720),
                length: 4,
                kind: fs_ext4::journal::DeletedContentRangeKind::RecoverableData,
                allocation_state: fs_ext4::journal::RecoveryAllocationState::Free,
                sha256: Some(sha256_hex(&[1, 2, 3, 4])),
            },
            fs_ext4::journal::DeletedContentRange {
                logical_offset: 4,
                filesystem_block: Some(31),
                filesystem_source_offset: Some(31_744),
                length: 4,
                kind: fs_ext4::journal::DeletedContentRangeKind::AllocatedData,
                allocation_state: fs_ext4::journal::RecoveryAllocationState::Allocated,
                sha256: None,
            },
        ],
        None,
    );

    let recovery = candidate_record("source-1", 2, &"a".repeat(64), &journal, candidate)
        .expect("map partial candidate");

    assert_eq!(recovery.completeness, "partial");
    assert_eq!(recovery.allocation_state, "partially_overwritten");
    assert_eq!(recovery.recoverable_bytes, 4);
    assert_eq!(recovery.ranges.len(), 2);
    assert!(recovery.content_sha256.is_none());
    assert!(recovery
        .warnings
        .iter()
        .any(|warning| warning.contains("currently allocated")));
}

fn candidate_with_content(
    declared_size: u64,
    data_allocation_state: fs_ext4::journal::RecoveryAllocationState,
    ranges: Vec<fs_ext4::journal::DeletedContentRange>,
    content_sha256: Option<String>,
) -> fs_ext4::journal::DeletedInodeCandidate {
    let recoverable_bytes = ranges
        .iter()
        .filter(|range| range.kind == fs_ext4::journal::DeletedContentRangeKind::RecoverableData)
        .map(|range| range.length)
        .sum();
    fs_ext4::journal::DeletedInodeCandidate {
        inode: 77,
        kind: fs_ext4::journal::DeletedInodeKind::RegularFile,
        mode: 0o100644,
        declared_size,
        deletion_time: 1_700_000_000,
        transaction_sequence: 9,
        descriptor_journal_block: 1,
        payload_journal_block: 0,
        inode_offset_within_payload: 44,
        journal_source_offset: 300,
        journal_source_length: 256,
        inode_table_group: 0,
        inode_table_block: 12,
        tag_marked_deleted: false,
        replay_revoked: false,
        journal_checksum_verified: true,
        inode_checksum_verified: false,
        completeness: if recoverable_bytes == declared_size {
            fs_ext4::journal::RecoveryCompleteness::Complete
        } else {
            fs_ext4::journal::RecoveryCompleteness::Partial
        },
        recoverable_bytes,
        content_mapping: fs_ext4::journal::DeletedContentMapping {
            state: fs_ext4::journal::DeletedContentMappingState::Mapped,
            inode_allocation_state: fs_ext4::journal::RecoveryAllocationState::Free,
            data_allocation_state,
            ranges,
            recoverable_bytes,
            content_md5: content_sha256.as_ref().map(|_| "a".repeat(32)),
            content_sha1: content_sha256.as_ref().map(|_| "b".repeat(40)),
            content_sha256,
            issue: None,
        },
        confidence: 0.9,
        recovery_method: "jbd2_inode_table_snapshot".to_string(),
    }
}
