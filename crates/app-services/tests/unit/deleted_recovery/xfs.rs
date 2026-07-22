use super::*;

#[test]
fn maps_only_explicit_xfs_deletion_proof_with_filesystem_relative_provenance() {
    let mut snapshot = vec![0u8; 1_024];
    snapshot[512..].fill(0x6b);
    let candidate = fs_xfs::log::XfsDeletedFileCandidate {
        inode: 42,
        record_lsn: (7u64 << 32) | 1,
        record_log_block: 1,
        record_source_offset: 0x20_0000,
        operation_index: 3,
        provenance: vec![fs_xfs::log::XfsLogSourceSpan {
            snapshot_offset: 512,
            source_offset: 0x20_0000,
            length: 512,
        }],
        proof: fs_xfs::log::XfsDeletionProof::InodeCoreNlinkZero,
        completeness: fs_xfs::log::XfsRecoveryCompleteness::MetadataOnly,
    };

    let recovery = deleted_candidate_record("source-1", 2, &"b".repeat(64), &snapshot, candidate)
        .expect("explicit deletion proof should map");

    assert_eq!(recovery.inode, "42");
    assert_eq!(recovery.recovery_method, "xfs_logged_inode_nlink_zero");
    assert_eq!(recovery.log_cycle, Some(7));
    assert_eq!(recovery.ranges.len(), 1);
    assert_eq!(recovery.ranges[0].source_kind, "filesystem");
    assert_eq!(recovery.ranges[0].source_offset, 0x20_0000);
    assert_eq!(recovery.ranges[0].length, 512);
    assert_eq!(
        recovery.ranges[0].sha256.as_deref(),
        Some(sha256_hex(&snapshot[512..]).as_str())
    );
    assert_eq!(recovery.completeness, "metadata_only");
    assert_eq!(recovery.recoverable_bytes, 0);
}
