use super::*;
use sha2::{Digest, Sha256};
use std::io::Cursor;

#[test]
fn reader_copy_hashes_and_verifies_the_logical_stream() {
    let temporary = tempfile::TempDir::new().unwrap();
    let destination = temporary.path().join("evidence.bin");
    let bytes = b"forensic export";
    let result = copy_reader_to_destination(
        &mut Cursor::new(bytes),
        Some(bytes.len() as u64),
        &destination,
        false,
    )
    .unwrap();

    assert_eq!(result.bytes_written, bytes.len() as u64);
    assert_eq!(result.sha256, hex::encode(Sha256::digest(bytes)));
    assert_eq!(std::fs::read(destination).unwrap(), bytes);
}

#[test]
fn size_mismatch_removes_temporary_output_and_does_not_publish() {
    let temporary = tempfile::TempDir::new().unwrap();
    let destination = temporary.path().join("truncated.bin");
    let error =
        copy_reader_to_destination(&mut Cursor::new(b"short"), Some(12), &destination, false)
            .unwrap_err();

    assert!(matches!(error, FileServiceError::Integrity(_)));
    assert!(!destination.exists());
    assert!(std::fs::read_dir(temporary.path())
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn overwrite_publishes_complete_replacement() {
    let temporary = tempfile::TempDir::new().unwrap();
    let destination = temporary.path().join("replacement.bin");
    std::fs::write(&destination, b"old").unwrap();

    let result = copy_reader_to_destination(
        &mut Cursor::new(b"new evidence"),
        Some(12),
        &destination,
        true,
    )
    .unwrap();

    assert_eq!(result.bytes_written, 12);
    assert_eq!(std::fs::read(destination).unwrap(), b"new evidence");
}

#[test]
fn chunked_copy_rejects_early_eof_without_publishing() {
    let temporary = tempfile::TempDir::new().unwrap();
    let destination = temporary.path().join("cephfs.bin");
    let error = copy_chunks_to_destination(8, &destination, false, |offset, _| {
        if offset == 0 {
            Ok(vec![1, 2, 3, 4])
        } else {
            Ok(Vec::new())
        }
    })
    .unwrap_err();

    assert!(matches!(error, FileServiceError::Integrity(_)));
    assert!(!destination.exists());
}

#[test]
fn chunked_copy_never_requests_more_than_one_mebibyte() {
    let temporary = tempfile::TempDir::new().unwrap();
    let destination = temporary.path().join("bounded-cephfs.bin");
    let source = vec![0x5a; 2 * 1024 * 1024 + 17];
    let mut requests = Vec::new();

    let result = copy_chunks_to_destination(
        source.len() as u64,
        &destination,
        false,
        |offset, length| {
            requests.push((offset, length));
            let start = offset as usize;
            let end = (start + length as usize).min(source.len());
            Ok(source[start..end].to_vec())
        },
    )
    .unwrap();

    assert_eq!(requests.len(), 3);
    assert!(requests.iter().all(|(_, length)| *length <= 1024 * 1024));
    assert_eq!(result.bytes_written, source.len() as u64);
    assert_eq!(result.sha256, hex::encode(Sha256::digest(&source)));
    assert_eq!(std::fs::read(destination).unwrap(), source);
}

#[test]
fn no_clobber_publish_preserves_existing_destination() {
    let temporary = tempfile::TempDir::new().unwrap();
    let destination = temporary.path().join("existing.bin");
    std::fs::write(&destination, b"original").unwrap();

    let error = copy_reader_to_destination(
        &mut Cursor::new(b"replacement"),
        Some(11),
        &destination,
        false,
    )
    .unwrap_err();

    assert!(matches!(error, FileServiceError::Io(_)));
    assert_eq!(std::fs::read(destination).unwrap(), b"original");
}
