use super::*;
use std::io::Cursor;

#[test]
fn test_extract_text_utf8() {
    let data = b"Hello, World!";
    let result = extract_text(Cursor::new(data), "file-1", None);
    assert!(result.extractable);
    assert_eq!(result.content, "Hello, World!");
    assert_eq!(result.encoding, "utf-8");
}

#[test]
fn test_extract_text_binary() {
    let data = b"Hello";
    let result = extract_text(
        Cursor::new(data),
        "file-1",
        Some("application/octet-stream"),
    );
    assert!(!result.extractable);
    assert_eq!(result.encoding, "binary");
}

#[test]
fn test_extract_text_empty() {
    let data = b"";
    let result = extract_text(Cursor::new(data), "file-1", None);
    assert!(result.extractable);
    assert_eq!(result.content, "");
}

#[test]
fn test_extract_text_json() {
    let data = b"{\"key\": \"value\"}";
    let result = extract_text(Cursor::new(data), "file-1", Some("application/json"));
    assert!(result.extractable);
}

#[test]
fn test_extracted_text_fields() {
    let data = b"test";
    let result = extract_text(Cursor::new(data), "file-1", None);
    assert_eq!(result.file_id, "file-1");
    assert_eq!(result.byte_count, 4);
}
