use super::*;
use std::io::Cursor;

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
