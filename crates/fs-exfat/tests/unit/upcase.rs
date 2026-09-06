use super::*;

#[test]
fn fallback_matches_ascii_without_collapsing_non_ascii() {
    let table = UpcaseTable::fallback();
    assert_eq!(table.fold("ReadMe"), table.fold("README"));
    assert_ne!(table.fold("a"), table.fold("b"));
    assert_eq!(table.fold("中文"), table.fold("中文"));
}

#[test]
fn expands_compressed_identity_table() {
    let mut bytes = Vec::with_capacity(UPCASE_CODEPOINTS * 2);
    for code in 0..=u16::MAX {
        bytes.extend_from_slice(&code.to_le_bytes());
    }
    let table = UpcaseTable::from_compressed(&bytes).unwrap();
    assert_eq!(
        table.fold("ReadMe"),
        "ReadMe".encode_utf16().collect::<Vec<_>>()
    );
}

#[test]
fn rejects_incomplete_compressed_table() {
    let error = UpcaseTable::from_compressed(&[0, 0]).unwrap_err();
    assert!(error.to_string().contains("does not cover"));
}
