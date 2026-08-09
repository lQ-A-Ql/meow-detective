use super::*;

fn index_entry(reference: u64, name: &str, flags: u32, file_flags: u32) -> Vec<u8> {
    let utf16 = name.encode_utf16().collect::<Vec<_>>();
    let mut entry = vec![0u8; 0x52 + utf16.len() * 2];
    entry[0..8].copy_from_slice(&reference.to_le_bytes());
    let entry_len = entry.len() as u16;
    entry[8..10].copy_from_slice(&entry_len.to_le_bytes());
    entry[0x0C..0x10].copy_from_slice(&flags.to_le_bytes());
    entry[0x48..0x4C].copy_from_slice(&file_flags.to_le_bytes());
    entry[0x50] = utf16.len() as u8;
    for (index, character) in utf16.iter().enumerate() {
        let offset = 0x52 + index * 2;
        entry[offset..offset + 2].copy_from_slice(&character.to_le_bytes());
    }
    entry
}

fn terminator_entry() -> Vec<u8> {
    let mut entry = vec![0u8; 0x52];
    entry[8..10].copy_from_slice(&0x52u16.to_le_bytes());
    entry[0x0C..0x10].copy_from_slice(&INDEX_ENTRY_TERMINATOR.to_le_bytes());
    entry
}

fn entry_list(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut list = Vec::new();
    for entry in entries {
        list.extend_from_slice(entry);
    }
    list.extend_from_slice(&terminator_entry());
    list
}

/// Build a 1 KiB INDX block (two sectors) whose entry region carries
/// `entries`, already in on-disk form (fixup stamped).
fn index_block(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut block = vec![0u8; 1024];
    block[0..4].copy_from_slice(b"INDX");
    block[4..6].copy_from_slice(&0x28u16.to_le_bytes());
    block[6..8].copy_from_slice(&3u16.to_le_bytes());
    let list = entry_list(entries);
    let used = 0x18 + list.len();
    block[0x18..0x1C].copy_from_slice(&0x18u32.to_le_bytes());
    block[0x1C..0x20].copy_from_slice(&(used as u32).to_le_bytes());
    block[0x20..0x24].copy_from_slice(&(used as u32).to_le_bytes());
    block[0x30..0x30 + list.len()].copy_from_slice(&list);
    let sequence = 0xA55Au16.to_le_bytes();
    block[0x28..0x2A].copy_from_slice(&sequence);
    for (sector, tail) in [510usize, 1022].iter().enumerate() {
        let usa = 0x2A + sector * 2;
        block[usa] = block[*tail];
        block[usa + 1] = block[*tail + 1];
        block[*tail] = sequence[0];
        block[*tail + 1] = sequence[1];
    }
    block
}

#[test]
fn find_entry_span_matches_name_case_insensitively() {
    let target = index_entry((3u64 << 48) | 77, "OSDATA", 0, 0x1000_0000);
    let other = index_entry((3u64 << 48) | 12, "SYSTEM", 0, 0);
    let list = entry_list(&[other, target]);
    let (start, len, reference, is_dir) = find_entry_span(&list, "osdata")
        .expect("lookup")
        .expect("entry found");
    let expected_len = 0x52 + 6 * 2;
    assert_eq!(start, 0x52 + 6 * 2);
    assert_eq!(len, expected_len);
    assert_eq!(reference, (3u64 << 48) | 77);
    assert!(is_dir);
}

#[test]
fn find_entry_span_stops_at_terminator() {
    let only_terminator = entry_list(&[]);
    assert!(find_entry_span(&only_terminator, "OSDATA")
        .expect("lookup")
        .is_none());
}

#[test]
fn find_entry_span_refuses_entry_with_child_sub_index() {
    let target = index_entry((3u64 << 48) | 9, "OSDATA", INDEX_ENTRY_HAS_CHILD, 0);
    let list = entry_list(&[target]);
    let error = find_entry_span(&list, "OSDATA").expect_err("must refuse");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn edit_index_block_removes_entry_and_shrinks_used_size() {
    let target = index_entry((3u64 << 48) | 77, "OSDATA", 0, 0x1000_0000);
    let keep = index_entry((3u64 << 48) | 12, "SYSTEM", 0, 0);
    let keep_len = keep.len();
    let target_len = target.len();
    let mut block = index_block(&[target, keep.clone()]);
    crate::utils::apply_record_fixup(&mut block, 512).expect("fixup applies");
    let used_before = u32::from_le_bytes(block[0x1C..0x20].try_into().expect("used"));

    let (reference, is_dir) = edit_index_block(&mut block, "OSDATA").expect("edit");
    assert_eq!(reference, (3u64 << 48) | 77);
    assert!(is_dir);
    let used_after = u32::from_le_bytes(block[0x1C..0x20].try_into().expect("used"));
    assert_eq!(used_after, used_before - target_len as u32);
    // The surviving entry shifted into the removed entry's slot and the
    // terminator still closes the list; the tail gap is zeroed.
    assert_eq!(&block[0x30..0x30 + keep_len], &keep[..]);
    let tail = 0x18 + used_before as usize;
    assert!(block[tail - target_len..tail].iter().all(|byte| *byte == 0));
    assert!(find_entry_span(
        &block[index_block_entries_region(&block).expect("region")],
        "OSDATA"
    )
    .expect("lookup")
    .is_none());
}

/// Build a 1 KiB FILE record carrying one resident $INDEX_ROOT whose entry
/// region holds `entries`; `used_override` corrupts the used-size field when
/// set.
fn index_root_record(entries: &[Vec<u8>], used_override: Option<u32>) -> Vec<u8> {
    let mut record = vec![0u8; 1024];
    record[0..4].copy_from_slice(b"FILE");
    record[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    let list = entry_list(entries);
    let content_len = 0x20 + list.len();
    let attr_pos = 0x38usize;
    let attr_len = 0x18 + content_len;
    record[attr_pos..attr_pos + 4].copy_from_slice(&ATTR_TYPE_INDEX_ROOT.to_le_bytes());
    record[attr_pos + 4..attr_pos + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
    // Resident flag (attr byte 8) stays zero.
    record[attr_pos + 0x10..attr_pos + 0x14].copy_from_slice(&(content_len as u32).to_le_bytes());
    record[attr_pos + 0x14..attr_pos + 0x16].copy_from_slice(&0x18u16.to_le_bytes());
    let header = attr_pos + 0x18 + 0x10;
    let used = used_override.unwrap_or(0x10 + list.len() as u32);
    record[header..header + 4].copy_from_slice(&0x10u32.to_le_bytes());
    record[header + 4..header + 8].copy_from_slice(&used.to_le_bytes());
    record[header + 0x10..header + 0x10 + list.len()].copy_from_slice(&list);
    record
}

#[test]
fn edit_index_root_removes_entry_and_shrinks_used_size() {
    let target = index_entry((3u64 << 48) | 77, "OSDATA", 0, 0x1000_0000);
    let keep = index_entry((3u64 << 48) | 12, "SYSTEM", 0, 0);
    let target_len = target.len();
    let mut record = index_root_record(&[target, keep.clone()], None);
    let header = 0x38 + 0x18 + 0x10;
    let used_before = u32::from_le_bytes(record[header + 4..header + 8].try_into().expect("used"));

    let (reference, is_dir) = edit_index_root(&mut record, "OSDATA").expect("edit");
    assert_eq!(reference, (3u64 << 48) | 77);
    assert!(is_dir);
    let used_after = u32::from_le_bytes(record[header + 4..header + 8].try_into().expect("used"));
    assert_eq!(used_after, used_before - target_len as u32);
    // The surviving entry shifted into the removed entry's slot and the tail
    // gap is zeroed.
    let region_start = header + 0x10;
    assert_eq!(&record[region_start..region_start + keep.len()], &keep[..]);
    let tail = header + used_before as usize;
    assert!(record[tail - target_len..tail]
        .iter()
        .all(|byte| *byte == 0));
}

#[test]
fn edit_index_root_rejects_used_size_past_attribute_content() {
    let target = index_entry((3u64 << 48) | 77, "OSDATA", 0, 0x1000_0000);
    // This used_size stays inside the MFT record but runs past the resident
    // attribute content: the edit must not shift the next attribute's bytes.
    let mut record = index_root_record(&[target], Some(900));
    let error = edit_index_root(&mut record, "OSDATA").expect_err("must refuse");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn inverse_record_fixup_round_trips_through_apply() {
    let target = index_entry((3u64 << 48) | 77, "OSDATA", 0, 0x1000_0000);
    let on_disk = index_block(&[target]);
    let mut logical = on_disk.clone();
    crate::utils::apply_record_fixup(&mut logical, 512).expect("fixup applies");
    edit_index_block(&mut logical, "OSDATA").expect("edit");
    let mut rewritten = logical.clone();
    inverse_record_fixup(&mut rewritten, 512).expect("inverse");
    // Re-applying the fixup to the rewritten image must validate the USA
    // and reproduce the exact logical content.
    let mut reapplied = rewritten;
    crate::utils::apply_record_fixup(&mut reapplied, 512).expect("re-apply");
    assert_eq!(reapplied, logical);
}
