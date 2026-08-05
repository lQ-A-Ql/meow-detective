use super::*;

fn index_entry(reference: u64, name: &str) -> Vec<u8> {
    let utf16 = name.encode_utf16().collect::<Vec<_>>();
    let mut entry = vec![0u8; 0x52 + utf16.len() * 2];
    entry[0..8].copy_from_slice(&reference.to_le_bytes());
    let entry_len = entry.len() as u16;
    entry[8..10].copy_from_slice(&entry_len.to_le_bytes());
    entry[0x50] = utf16.len() as u8;
    entry[0x51] = 1;
    for (index, character) in utf16.iter().enumerate() {
        let offset = 0x52 + index * 2;
        entry[offset..offset + 2].copy_from_slice(&character.to_le_bytes());
    }
    entry
}

fn index_record(vbn: u64, name: &str) -> Vec<u8> {
    let mut record = vec![0u8; 512];
    record[0..4].copy_from_slice(b"INDX");
    record[4..6].copy_from_slice(&0x28u16.to_le_bytes());
    record[6..8].copy_from_slice(&2u16.to_le_bytes());
    record[0x10..0x18].copy_from_slice(&vbn.to_le_bytes());
    let entry = index_entry((3u64 << 48) | 42, name);
    let entries_offset = 0x18u32;
    let entries_size = entries_offset + entry.len() as u32;
    record[0x18..0x1c].copy_from_slice(&entries_offset.to_le_bytes());
    record[0x1c..0x20].copy_from_slice(&entries_size.to_le_bytes());
    record[0x20..0x24].copy_from_slice(&entries_size.to_le_bytes());
    record[0x30..0x30 + entry.len()].copy_from_slice(&entry);

    let update_sequence = 0xA55Au16;
    record[0x28..0x2a].copy_from_slice(&update_sequence.to_le_bytes());
    let original_tail = [record[510], record[511]];
    record[0x2a..0x2c].copy_from_slice(&original_tail);
    record[510..512].copy_from_slice(&update_sequence.to_le_bytes());
    record
}

#[test]
fn bitmap_skips_unallocated_index_record() {
    let mut allocation = index_record(0, "Active");
    allocation.extend(vec![0xCC; 512]);

    let entries = parse_index_allocation(&allocation, &[0b0000_0001], 512, 512, 512).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].node.name, "Active");
    assert_eq!(entries[0].mft_sequence, 3);
}

#[test]
fn update_sequence_mismatch_rejects_index_record() {
    let mut record = index_record(0, "Broken");
    record[510..512].copy_from_slice(&0xFFFFu16.to_le_bytes());

    let error = parse_index_allocation(&record, &[1], 512, 512, 512).unwrap_err();

    assert!(error
        .to_string()
        .contains("update sequence signature mismatch"));
}

#[test]
fn vbn_mismatch_rejects_index_record() {
    let record = index_record(9, "WrongVbn");

    let error = parse_index_allocation(&record, &[1], 512, 512, 512).unwrap_err();

    assert!(error.to_string().contains("VBN mismatch"));
}

#[test]
fn subcluster_index_records_use_512_byte_vbn_units() {
    let mut allocation = vec![0u8; 1024];
    allocation.extend(index_record_1024(2, "Second"));

    let entries = parse_index_allocation(&allocation, &[0b0000_0010], 1024, 512, 4096).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].node.name, "Second");
}

fn index_record_1024(vbn: u64, name: &str) -> Vec<u8> {
    let mut record = vec![0u8; 1024];
    record[0..4].copy_from_slice(b"INDX");
    record[4..6].copy_from_slice(&0x28u16.to_le_bytes());
    record[6..8].copy_from_slice(&3u16.to_le_bytes());
    record[0x10..0x18].copy_from_slice(&vbn.to_le_bytes());
    let entry = index_entry((3u64 << 48) | 42, name);
    let entries_offset = 0x18u32;
    let entries_size = entries_offset + entry.len() as u32;
    record[0x18..0x1c].copy_from_slice(&entries_offset.to_le_bytes());
    record[0x1c..0x20].copy_from_slice(&entries_size.to_le_bytes());
    record[0x20..0x24].copy_from_slice(&entries_size.to_le_bytes());
    record[0x30..0x30 + entry.len()].copy_from_slice(&entry);

    let update_sequence = 0xA55Au16;
    record[0x28..0x2a].copy_from_slice(&update_sequence.to_le_bytes());
    for (index, tail) in [510usize, 1022].into_iter().enumerate() {
        let replacement = 0x2a + index * 2;
        let original = [record[tail], record[tail + 1]];
        record[replacement..replacement + 2].copy_from_slice(&original);
        record[tail..tail + 2].copy_from_slice(&update_sequence.to_le_bytes());
    }
    record
}
