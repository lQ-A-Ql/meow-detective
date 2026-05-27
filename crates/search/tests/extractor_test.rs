use search::extract_text;

#[test]
fn extract_utf8_text() {
    let data = b"Hello, forensic world!";
    let result = extract_text(&data[..], "file-1", Some("text/plain"));
    assert!(result.extractable);
    assert_eq!(result.content, "Hello, forensic world!");
    assert_eq!(result.encoding, "utf-8");
}

#[test]
fn extract_utf16_le_text() {
    let mut data = vec![0xFFu8, 0xFE];
    for c in "test".encode_utf16() {
        data.extend_from_slice(&c.to_le_bytes());
    }
    let result = extract_text(&data[..], "file-2", None);
    assert!(result.extractable);
    assert!(result.content.contains("test"));
    assert_eq!(result.encoding, "utf-16le");
}

#[test]
fn binary_file_not_extractable() {
    let data = [0x00, 0x01, 0x02, 0xFF, 0xFE];
    let result = extract_text(&data[..], "file-3", Some("application/octet-stream"));
    assert!(!result.extractable);
    assert_eq!(result.encoding, "binary");
}

#[test]
fn empty_file_returns_empty() {
    let data: &[u8] = &[];
    let result = extract_text(data, "empty", Some("text/plain"));
    assert!(result.extractable);
    assert_eq!(result.content, "");
}
