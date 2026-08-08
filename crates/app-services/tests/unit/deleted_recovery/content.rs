use std::io::Cursor;

use persistence_sqlite::repositories::deleted_recovery_repo::{
    DeletedRecoveryRecord, RecoveryRangeRecord,
};
use sha2::{Digest, Sha256};

use super::*;

#[test]
fn partial_recovery_reads_only_a_contiguous_verified_window() {
    let source = b"abcdefgh----ijklmnop".to_vec();
    let recovery = recovery(
        "partial",
        20,
        None,
        vec![
            content_range(0, 0, b"abcdefgh"),
            content_range(12, 12, b"ijklmnop"),
        ],
    );
    let mut reader = Cursor::new(source);

    let first = read_verified_content(&mut reader, &recovery, 2, 4).unwrap();
    assert_eq!(first.bytes, b"cdef");
    assert_eq!(first.verified_range_ordinals, vec![1]);

    let error = read_verified_content(&mut reader, &recovery, 6, 4)
        .unwrap_err()
        .to_string();
    assert!(error.contains("unrecovered gap"));
}

#[test]
fn complete_recovery_can_read_across_verified_source_ranges() {
    let source = b"abcdefgh----ijklmnop".to_vec();
    let complete_hash = hash(b"abcdefghijklmnop");
    let recovery = recovery(
        "complete",
        16,
        Some(complete_hash),
        vec![
            content_range(0, 0, b"abcdefgh"),
            content_range(8, 12, b"ijklmnop"),
        ],
    );
    let mut reader = Cursor::new(source);

    let read = read_verified_content(&mut reader, &recovery, 6, 6).unwrap();
    assert_eq!(read.bytes, b"ghijkl");
    assert_eq!(read.end, 12);
    assert_eq!(read.verified_range_ordinals, vec![1, 2]);
}

#[test]
fn persisted_range_digest_is_rechecked_before_bytes_are_returned() {
    let mut source = b"abcdefgh".to_vec();
    let recovery = recovery(
        "complete",
        8,
        Some(hash(b"abcdefgh")),
        vec![content_range(0, 0, b"abcdefgh")],
    );
    source[3] = b'X';
    let mut reader = Cursor::new(source);

    let error = read_verified_content(&mut reader, &recovery, 0, 4)
        .unwrap_err()
        .to_string();
    assert!(error.contains("SHA-256"));
}

fn recovery(
    completeness: &str,
    declared_size: u64,
    content_sha256: Option<String>,
    mut ranges: Vec<RecoveryRangeRecord>,
) -> DeletedRecoveryRecord {
    for (index, range) in ranges.iter_mut().enumerate() {
        range.ordinal = u32::try_from(index + 1).unwrap();
    }
    DeletedRecoveryRecord {
        id: format!("recovery:{}", "a".repeat(64)),
        inode: "77".to_string(),
        original_path: None,
        entry_type: Some("file".to_string()),
        mode: Some(0o100644),
        mft_sequence: None,
        deleted_at_unix: Some(1_700_000_000),
        declared_size,
        recoverable_bytes: ranges.iter().map(|range| range.length).sum(),
        completeness: completeness.to_string(),
        recovery_method: "test".to_string(),
        confidence: 1.0,
        allocation_state: "free".to_string(),
        transaction_id: None,
        log_sequence: None,
        log_cycle: None,
        content_md5: None,
        content_sha1: None,
        content_sha256,
        warnings: Vec::new(),
        ranges,
    }
}

fn content_range(logical_offset: u64, source_offset: u64, bytes: &[u8]) -> RecoveryRangeRecord {
    RecoveryRangeRecord {
        ordinal: 0,
        range_role: "content".to_string(),
        source_kind: "filesystem".to_string(),
        logical_offset,
        source_offset,
        physical_offset: None,
        length: bytes.len() as u64,
        allocation_state: "free".to_string(),
        sha256: Some(hash(bytes)),
    }
}

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
