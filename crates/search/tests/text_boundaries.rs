use search::{classify_text_bytes, extract_text, TextContentStatus};
#[test]
fn valid_utf8_split_by_the_sniff_boundary_remains_indexable() {
    let mut bytes = vec![b'a'; 65535];
    bytes.extend_from_slice("é".as_bytes());
    assert_eq!(
        classify_text_bytes(&bytes, None),
        TextContentStatus::TextUtf8
    );
    let text = extract_text(std::io::Cursor::new(bytes), "file", None);
    assert!(text.extractable);
    assert!(text.content.ends_with('é'));
}
#[test]
fn utf16_surrogate_pairs_may_cross_the_sniff_boundary() {
    let mut bytes = vec![0xff, 0xfe];
    for _ in 0..32766 {
        bytes.extend_from_slice(&0x61u16.to_le_bytes());
    }
    for unit in "🦀".encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    assert_eq!(
        classify_text_bytes(&bytes, None),
        TextContentStatus::TextUtf16Le
    );
    let text = extract_text(std::io::Cursor::new(bytes), "file", None);
    assert!(text.extractable);
    assert!(text.content.ends_with('🦀'));
}
#[test]
fn empty_utf16_is_text_but_odd_or_unpaired_final_data_is_not() {
    assert!(extract_text(std::io::Cursor::new([0xff, 0xfe]), "file", None).extractable);
    for bytes in [vec![0xff, 0xfe, 0x41], vec![0xff, 0xfe, 0x3e, 0xd8]] {
        let text = extract_text(std::io::Cursor::new(bytes), "file", None);
        assert!(!text.extractable);
        assert_eq!(text.encoding, "unsupported_encoding");
    }
}
