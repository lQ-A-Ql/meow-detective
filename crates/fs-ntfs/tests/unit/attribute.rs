use super::*;

#[test]
fn attribute_list_entry_preserves_full_identity() {
    let mut bytes = vec![0u8; 0x20];
    bytes[0..4].copy_from_slice(&0x80u32.to_le_bytes());
    bytes[4..6].copy_from_slice(&0x20u16.to_le_bytes());
    bytes[8..0x10].copy_from_slice(&9u64.to_le_bytes());
    let reference = (7u64 << 48) | 42;
    bytes[0x10..0x18].copy_from_slice(&reference.to_le_bytes());
    bytes[0x18..0x1a].copy_from_slice(&3u16.to_le_bytes());

    let entries = parse_attribute_list_entries(&bytes).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].record_number, 42);
    assert_eq!(entries[0].record_sequence, 7);
    assert_eq!(entries[0].attribute_id, 3);
    assert_eq!(entries[0].lowest_vcn, 9);
}

#[test]
fn malformed_attribute_list_entry_is_rejected() {
    let mut bytes = vec![0u8; 0x20];
    bytes[0..4].copy_from_slice(&0x80u32.to_le_bytes());
    bytes[4..6].copy_from_slice(&0x40u16.to_le_bytes());

    let error = parse_attribute_list_entries(&bytes).unwrap_err();

    assert!(error.to_string().contains("entry length"));
}
