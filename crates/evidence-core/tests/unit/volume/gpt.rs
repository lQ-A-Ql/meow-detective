use super::*;

#[test]
fn parse_gpt_entries_with_minimum_entry_size() {
    // Create a minimal GPT entry (128 bytes)
    let mut entry = vec![0u8; 128];
    // Set type GUID (first 16 bytes)
    entry[0..16].copy_from_slice(&MS_BASIC_DATA);
    // Set start LBA at offset 32
    entry[32..40].copy_from_slice(&100u64.to_le_bytes());
    // Set end LBA at offset 40
    entry[40..48].copy_from_slice(&200u64.to_le_bytes());
    // Set name "Test" in UTF-16LE at offset 56
    entry[56] = b'T';
    entry[57] = 0;
    entry[58] = b'e';
    entry[59] = 0;
    entry[60] = b's';
    entry[61] = 0;
    entry[62] = b't';
    entry[63] = 0;

    let parts = parse_gpt_entries(&entry, 128, 1);
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].name, "Test");
    assert_eq!(parts[0].start_lba, 100);
    assert_eq!(parts[0].end_lba, 200);
}

#[test]
fn parse_gpt_entries_with_larger_entry_size() {
    // Entry size > 128 should still work
    let mut entry = vec![0u8; 256];
    entry[0..16].copy_from_slice(&MS_BASIC_DATA);
    entry[32..40].copy_from_slice(&100u64.to_le_bytes());
    entry[40..48].copy_from_slice(&200u64.to_le_bytes());

    let parts = parse_gpt_entries(&entry, 256, 1);
    assert_eq!(parts.len(), 1);
}

#[test]
fn parse_gpt_entries_rejects_small_entry_size() {
    let entry = vec![0u8; 64];
    let parts = parse_gpt_entries(&entry, 64, 1);
    assert!(parts.is_empty());
}

#[test]
fn parse_gpt_entries_skips_empty_partitions() {
    let entry = vec![0u8; 128];
    // start=0, end=0 means empty partition
    let parts = parse_gpt_entries(&entry, 128, 1);
    assert!(parts.is_empty());
}

#[test]
fn format_guid_basic() {
    let guid = MS_BASIC_DATA;
    let formatted = format_guid(&guid);
    // Should contain dashes
    assert!(formatted.contains('-'));
    assert_eq!(formatted.len(), 36); // XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX
}
