use super::*;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn sha256_reader_basic() {
    let data = b"test data for hashing";
    let mut cursor = Cursor::new(data);
    let hash = HashService::sha256_reader(&mut cursor).unwrap();
    assert_eq!(hash, HashService::sha256_bytes(data));
}

#[test]
fn sha256_bytes_hello_world() {
    let hash = HashService::sha256_bytes(b"hello world");
    assert_eq!(
        hash,
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

#[test]
fn verify_sha256_correct() {
    let data = b"evidence data";
    let hash = HashService::sha256_bytes(data);
    assert!(HashService::verify_sha256(data, &hash));
}

#[test]
fn verify_sha256_incorrect() {
    assert!(!HashService::verify_sha256(
        b"hello",
        &HashService::sha256_bytes(b"world")
    ));
}

#[test]
fn sha256_file_nonexistent() {
    let result = HashService::sha256_file(Path::new("/nonexistent/file"));
    assert!(result.is_err());
}

#[test]
fn hash_evidence_raw_is_standard_sha256_and_reports_backend() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("evidence.raw");
    std::fs::write(&path, b"raw evidence").unwrap();
    let cancelled = AtomicBool::new(false);
    let progress = AtomicBool::new(false);
    let result =
        HashService::hash_evidence(&path, &domain::DataSourceKind::Raw, &cancelled, &|_, _| {
            progress.store(true, Ordering::Release)
        })
        .unwrap();
    assert_eq!(result.digest, HashService::sha256_bytes(b"raw evidence"));
    assert_eq!(result.bytes_processed, 12);
    assert_eq!(result.parallel_segments, 1);
    assert_eq!(
        result.worker_threads,
        infrastructure::hashing::sha256_pipeline_worker_threads()
    );
    assert!(matches!(result.acceleration, "sha-ni" | "portable"));
    assert!(progress.load(Ordering::Acquire));
}

#[test]
fn hash_evidence_e01_uses_deterministic_manifest_for_multiple_segments() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("sample.E01");
    let second = directory.path().join("sample.E02");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let cancelled = AtomicBool::new(false);
    let result =
        HashService::hash_evidence(&first, &domain::DataSourceKind::E01, &cancelled, &|_, _| {})
            .unwrap();
    let manifest = format!(
        "segment=00000000;length=5;sha256={}\nsegment=00000001;length=6;sha256={}\n",
        HashService::sha256_bytes(b"first"),
        HashService::sha256_bytes(b"second")
    );
    assert_eq!(
        result.digest,
        HashService::sha256_bytes(manifest.as_bytes())
    );
    assert_eq!(result.bytes_processed, 11);
    assert_eq!(result.parallel_segments, 2);
    assert!(result.worker_threads >= 1);
}

#[test]
fn hash_evidence_honors_cancellation_before_reading() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("evidence.raw");
    std::fs::write(&path, b"evidence").unwrap();
    let cancelled = AtomicBool::new(true);
    let error =
        HashService::hash_evidence(&path, &domain::DataSourceKind::Raw, &cancelled, &|_, _| {})
            .unwrap_err();
    assert!(matches!(error, EvidenceHashError::Cancelled));
}
