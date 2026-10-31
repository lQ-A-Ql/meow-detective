use super::*;
use std::io::Cursor;

#[test]
fn sha256_empty() {
    assert_eq!(
        sha256_bytes(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn sha256_hello_world() {
    assert_eq!(
        sha256_bytes(b"hello world"),
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

#[test]
fn sha256_reader_basic() {
    let data = b"test data for hashing";
    let mut cursor = Cursor::new(data);
    let hash = sha256_reader(&mut cursor).unwrap();
    assert_eq!(hash, sha256_bytes(data));
}

#[test]
fn sha256_reader_empty() {
    let mut cursor = Cursor::new(b"");
    let hash = sha256_reader(&mut cursor).unwrap();
    assert_eq!(hash, sha256_bytes(b""));
}

#[test]
fn verify_sha256_match() {
    assert!(verify_sha256(b"hello", &sha256_bytes(b"hello")));
}

#[test]
fn verify_sha256_mismatch() {
    assert!(!verify_sha256(b"hello", &sha256_bytes(b"world")));
}

#[test]
fn verify_sha256_empty() {
    assert!(verify_sha256(b"", &sha256_bytes(b"")));
}

#[test]
fn sha256_large_data() {
    let data = vec![0xABu8; 100_000];
    let mut cursor = Cursor::new(&data);
    let hash = sha256_reader(&mut cursor).unwrap();
    assert_eq!(hash, sha256_bytes(&data));
}

#[test]
fn sha256_pipeline_matches_streaming_across_reader_lanes() {
    for length in [
        0u64,
        1,
        (8 * 1024 * 1024) - 1,
        8 * 1024 * 1024,
        (8 * 1024 * 1024) + 17,
        (64 * 1024 * 1024) + 29,
    ] {
        let file = tempfile::NamedTempFile::new().unwrap();
        file.as_file().set_len(length).unwrap();
        let pipelined = pipeline::sha256_file_pipelined_with_cancel(file.path(), &|| false, |_| {})
            .unwrap()
            .unwrap();
        let mut reader = file.reopen().unwrap();
        let streamed =
            digest_reader_with_cancel(&mut reader, HashAlgorithm::Sha256, || false, |_| {})
                .unwrap()
                .unwrap();
        assert_eq!(pipelined, streamed, "length={length}");
    }
}

#[test]
fn sha256_pipeline_stops_after_cancellation() {
    let file = tempfile::NamedTempFile::new().unwrap();
    file.as_file().set_len((64 * 1024 * 1024) + 1).unwrap();
    let checks = std::sync::atomic::AtomicUsize::new(0);

    let result = pipeline::sha256_file_pipelined_with_cancel(
        file.path(),
        &|| checks.fetch_add(1, std::sync::atomic::Ordering::AcqRel) >= 2,
        |_| {},
    )
    .unwrap();

    assert!(result.is_none());
}

#[test]
fn sha256_pipeline_propagates_reader_failure() {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let (recycle, _recycled) = std::sync::mpsc::sync_channel(1);
    sender
        .send(Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "injected read failure",
        )))
        .unwrap();
    let lanes = [pipeline::ReaderLane { receiver, recycle }];

    let error = pipeline::hash_reader_lanes(1, &lanes, &|| false, &mut |_| {}).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
}

#[test]
fn sha256_pipeline_reports_a_stopped_buffer_recycler() {
    let buffer = vec![0u8; pipeline::PIPELINE_BUFFER_BYTES];
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    sender.send(Ok(buffer)).unwrap();
    let (recycle, recycled) = std::sync::mpsc::sync_channel(0);
    drop(recycled);
    let lanes = [pipeline::ReaderLane { receiver, recycle }];

    let error = pipeline::hash_reader_lanes(
        (pipeline::PIPELINE_BUFFER_BYTES as u64) + 1,
        &lanes,
        &|| false,
        &mut |_| {},
    )
    .unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
}
