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

#[test]
fn classify_text_bytes_is_bounded_and_rejects_binary() {
    assert_eq!(
        classify_text_bytes(b"name: value\n", None),
        TextContentStatus::TextUtf8
    );
    assert_eq!(
        classify_text_bytes(&[0xFF, 0xFE, b'a', 0, b'\n', 0], None),
        TextContentStatus::TextUtf16Le
    );
    assert_eq!(
        classify_text_bytes(&[0, 1, 2, 0, 3, 4], None),
        TextContentStatus::Binary
    );
    let mut large = vec![b'a'; CONTENT_SNIFF_BYTES + 10];
    large[CONTENT_SNIFF_BYTES] = 0;
    assert_eq!(
        classify_text_bytes(&large, None),
        TextContentStatus::TextUtf8
    );
}

#[test]
fn extract_text_reports_unsupported_encoding_without_lossy_replacement() {
    let result = extract_text(Cursor::new([0xFF, 0x80, 0xFE]), "file-unsupported", None);
    assert!(!result.extractable);
    assert_eq!(result.encoding, "unsupported_encoding");
}
