use persistence_sqlite::repositories::deleted_recovery_repo::RecoveryRangeRecord;
use sha2::{Digest, Sha256};

use super::*;

fn content_range(ordinal: u32, logical_offset: u64, length: u64) -> RecoveryRangeRecord {
    RecoveryRangeRecord {
        ordinal,
        range_role: "content".to_string(),
        source_kind: "filesystem".to_string(),
        logical_offset,
        source_offset: logical_offset + 0x1000,
        physical_offset: Some(logical_offset + 0x1000),
        length,
        allocation_state: "free".to_string(),
        sha256: Some("a".repeat(64)),
    }
}

fn hasher_for(bytes: &[u8]) -> Sha256 {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
}

#[test]
fn content_claim_requires_contiguous_free_ranges_for_complete_recovery() {
    let ranges = vec![content_range(2, 0, 4), content_range(3, 4, 4)];
    let (allocation_state, completeness, digest) =
        content_claim(8, 8, true, hasher_for(b"12345678"), &ranges);

    assert_eq!(allocation_state, "free");
    assert_eq!(completeness, "complete");
    assert_eq!(digest, Some(hex::encode(Sha256::digest(b"12345678"))));
}

#[test]
fn content_claim_downgrades_gapped_or_partially_covered_ranges() {
    let ranges = vec![content_range(1, 0, 4), content_range(2, 8, 4)];
    let (allocation_state, completeness, digest) =
        content_claim(12, 8, false, hasher_for(b"12345678"), &ranges);

    assert_eq!(allocation_state, "partially_overwritten");
    assert_eq!(completeness, "partial");
    assert_eq!(digest, None);
}

#[test]
fn content_claim_does_not_invent_content_when_bitmap_verification_yields_no_ranges() {
    let (allocation_state, completeness, digest) = content_claim(4, 0, false, hasher_for(&[]), &[]);

    assert_eq!(allocation_state, "unverified");
    assert_eq!(completeness, "metadata_only");
    assert_eq!(digest, None);
}

#[test]
fn complete_empty_file_has_a_stable_empty_content_digest() {
    let (allocation_state, completeness, digest) = content_claim(0, 0, true, hasher_for(&[]), &[]);

    assert_eq!(allocation_state, "free");
    assert_eq!(completeness, "complete");
    assert_eq!(digest, Some(hex::encode(Sha256::digest([]))));
}
