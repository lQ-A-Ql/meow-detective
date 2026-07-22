use std::io::Cursor;

use persistence_sqlite::repositories::deleted_recovery_repo::{
    DeletedRecoveryRecord, RecoveryRangeRecord,
};
use sha2::{Digest, Sha256};

use super::*;

#[test]
fn complete_recovery_is_exported_atomically_with_verified_digest() {
    let directory = tempfile::TempDir::new().unwrap();
    let destination = directory.path().join("recovered.bin");
    let content = b"recoverable-content";
    let recovery = recovery("complete", content, Some(hash(content)));
    let mut reader = Cursor::new(content.to_vec());

    let outcome = export_complete_content(&mut reader, &recovery, &destination, false).unwrap();

    assert_eq!(outcome.bytes_written, content.len() as u64);
    assert_eq!(outcome.sha256, hash(content));
    assert_eq!(std::fs::read(destination).unwrap(), content);
}

#[test]
fn partial_recovery_is_not_exported_as_a_complete_file() {
    let directory = tempfile::TempDir::new().unwrap();
    let destination = directory.path().join("partial.bin");
    let content = b"partial";
    let recovery = recovery("partial", content, None);
    let mut reader = Cursor::new(content.to_vec());

    let error = export_complete_content(&mut reader, &recovery, &destination, false)
        .unwrap_err()
        .to_string();

    assert!(error.contains("only complete recovery candidates"));
    assert!(!destination.exists());
}

#[test]
fn digest_mismatch_leaves_no_destination_or_temporary_file() {
    let directory = tempfile::TempDir::new().unwrap();
    let destination = directory.path().join("corrupt.bin");
    let expected = b"expected";
    let recovery = recovery("complete", expected, Some(hash(expected)));
    let mut reader = Cursor::new(b"tampered".to_vec());

    let error = export_complete_content(&mut reader, &recovery, &destination, false)
        .unwrap_err()
        .to_string();

    assert!(error.contains("SHA-256"));
    assert!(!destination.exists());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn overwrite_replaces_the_destination_atomically() {
    let directory = tempfile::TempDir::new().unwrap();
    let destination = directory.path().join("existing.bin");
    std::fs::write(&destination, b"old-content").unwrap();
    let content = b"replacement-content";
    let recovery = recovery("complete", content, Some(hash(content)));
    let mut reader = Cursor::new(content.to_vec());

    let outcome = export_complete_content(&mut reader, &recovery, &destination, true).unwrap();

    assert_eq!(outcome.sha256, hash(content));
    assert_eq!(std::fs::read(destination).unwrap(), content);
}

fn recovery(
    completeness: &str,
    content: &[u8],
    content_sha256: Option<String>,
) -> DeletedRecoveryRecord {
    DeletedRecoveryRecord {
        id: format!("recovery:{}", "a".repeat(64)),
        inode: "77".to_string(),
        original_path: None,
        entry_type: Some("file".to_string()),
        mode: Some(0o100644),
        mft_sequence: None,
        deleted_at_unix: Some(1_700_000_000),
        declared_size: content.len() as u64,
        recoverable_bytes: content.len() as u64,
        completeness: completeness.to_string(),
        recovery_method: "test".to_string(),
        confidence: 1.0,
        allocation_state: "free".to_string(),
        transaction_id: None,
        log_sequence: None,
        log_cycle: None,
        content_sha256,
        warnings: Vec::new(),
        ranges: vec![RecoveryRangeRecord {
            ordinal: 1,
            range_role: "content".to_string(),
            source_kind: "filesystem".to_string(),
            logical_offset: 0,
            source_offset: 0,
            physical_offset: None,
            length: content.len() as u64,
            allocation_state: "free".to_string(),
            sha256: Some(hash(content)),
        }],
    }
}

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
