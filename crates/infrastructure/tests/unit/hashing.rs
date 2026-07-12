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
