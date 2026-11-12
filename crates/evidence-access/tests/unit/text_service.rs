use super::*;
use std::io::Cursor;

#[test]
fn detect_utf8() {
    let data = "Hello World".as_bytes();
    let info = TextService::detect_encoding(data);
    assert_eq!(info.name, "UTF-8");
    assert!(info.confidence > 0.9);
}

#[test]
fn detect_utf8_bom() {
    let data = [0xEF, 0xBB, 0xBF, b'H', b'e', b'l', b'l', b'o'];
    let info = TextService::detect_encoding(&data);
    assert_eq!(info.name, "UTF-8 BOM");
    assert_eq!(info.confidence, 1.0);
}

#[test]
fn detect_utf16_le() {
    let data = [0xFF, 0xFE, 0x48, 0x00, 0x65, 0x00]; // "He" in UTF-16 LE
    let info = TextService::detect_encoding(&data);
    assert_eq!(info.name, "UTF-16 LE");
}

#[test]
fn detect_gbk() {
    // "你好" in GBK
    let data = [0xC4, 0xE3, 0xBA, 0xC3];
    let info = TextService::detect_encoding(&data);
    assert_eq!(info.name, "GBK");
}

#[test]
fn is_likely_text_true() {
    let data = "Hello World\nThis is text".as_bytes();
    assert!(TextService::is_likely_text(data));
}

#[test]
fn is_likely_text_false() {
    let data = [0x00, 0x01, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00];
    assert!(!TextService::is_likely_text(&data));
}

#[test]
fn extract_text_preview_utf8() {
    let data = "Hello\nWorld\nTest".as_bytes();
    let mut cursor = Cursor::new(data);
    let preview = TextService::extract_text_preview(&mut cursor, 1024).unwrap();
    assert_eq!(preview.content, "Hello\nWorld\nTest");
    assert_eq!(preview.encoding, "UTF-8");
    assert_eq!(preview.line_count, 3);
    assert!(!preview.is_binary);
}

#[test]
fn extract_text_preview_binary() {
    let data = [0x00, 0x01, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00];
    let mut cursor = Cursor::new(&data);
    let preview = TextService::extract_text_preview(&mut cursor, 1024).unwrap();
    assert!(preview.is_binary);
}

#[test]
fn get_language_from_extension_js() {
    assert_eq!(
        TextService::get_language_from_extension("js"),
        Some("javascript")
    );
    assert_eq!(
        TextService::get_language_from_extension("JS"),
        Some("javascript")
    );
}

#[test]
fn get_language_from_extension_unknown() {
    assert_eq!(TextService::get_language_from_extension("xyz"), None);
}

#[test]
fn detect_utf16_be() {
    let data = [0xFE, 0xFF, 0x00, 0x48, 0x00, 0x65]; // "He" in UTF-16 BE
    let info = TextService::detect_encoding(&data);
    assert_eq!(info.name, "UTF-16 BE");
}

#[test]
fn extract_text_preview_truncated() {
    let data = "Hello World".repeat(1000);
    let mut cursor = Cursor::new(data.as_bytes());
    let preview = TextService::extract_text_preview(&mut cursor, 100).unwrap();
    assert!(preview.is_truncated);
}

#[test]
fn extract_text_preview_empty() {
    let data = b"";
    let mut cursor = Cursor::new(data);
    let preview = TextService::extract_text_preview(&mut cursor, 1024).unwrap();
    assert!(!preview.is_binary);
    // Empty string has 0 lines when split by newline
    assert_eq!(preview.line_count, 0);
}

#[test]
fn get_language_all_supported() {
    let test_cases = vec![
        ("js", "javascript"),
        ("ts", "typescript"),
        ("py", "python"),
        ("rs", "rust"),
        ("go", "go"),
        ("java", "java"),
        ("c", "c"),
        ("cpp", "cpp"),
        ("html", "html"),
        ("css", "css"),
        ("json", "json"),
        ("xml", "xml"),
        ("yaml", "yaml"),
        ("sql", "sql"),
        ("sh", "shell"),
        ("md", "markdown"),
    ];
    for (ext, expected_lang) in test_cases {
        assert_eq!(
            TextService::get_language_from_extension(ext),
            Some(expected_lang),
            "Extension '{}' should map to '{}'",
            ext,
            expected_lang
        );
    }
}

#[test]
fn is_likely_text_empty() {
    assert!(TextService::is_likely_text(b""));
}

#[test]
fn decode_text_utf8() {
    let data = "Hello 世界".as_bytes();
    let text = TextService::decode_text(data, UTF_8);
    assert_eq!(text, "Hello 世界");
}
