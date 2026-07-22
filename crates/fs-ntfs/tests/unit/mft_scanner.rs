use super::*;
use chrono::Datelike;

fn make_test_record(_record_number: u64, name: &str, parent: u64, is_dir: bool) -> Vec<u8> {
    let mut rec = vec![0u8; 1024];
    // FILE magic
    rec[0..4].copy_from_slice(b"FILE");
    // usa_offset=0, usa_count=0 → no fixup needed
    rec[4..6].copy_from_slice(&0u16.to_le_bytes());
    rec[6..8].copy_from_slice(&0u16.to_le_bytes());

    // Attribute offset
    let attr_off = 56u16;
    rec[0x14..0x16].copy_from_slice(&attr_off.to_le_bytes());

    // Flags: bit 0 = in use, bit 1 = directory
    let flags: u16 = if is_dir { 0x03 } else { 0x01 };
    rec[0x16..0x18].copy_from_slice(&flags.to_le_bytes());
    rec[0x10..0x12].copy_from_slice(&7u16.to_le_bytes());

    let mut pos = attr_off as usize;

    // $STANDARD_INFORMATION (0x10) — resident
    let si_content_size = 0x30u32;
    let si_attr_len = 0x60u32;
    rec[pos..pos + 4].copy_from_slice(&0x10u32.to_le_bytes());
    rec[pos + 4..pos + 8].copy_from_slice(&si_attr_len.to_le_bytes());
    rec[pos + 8] = 0; // resident
    rec[pos + 0x10..pos + 0x14].copy_from_slice(&si_content_size.to_le_bytes());
    rec[pos + 0x14..pos + 0x16].copy_from_slice(&0x18u16.to_le_bytes());
    // NTFS time for ~2005-01-01: 127111680000000000
    let test_time: u64 = 127_111_680_000_000_000;
    let content_start = pos + 0x18;
    rec[content_start..content_start + 8].copy_from_slice(&test_time.to_le_bytes());
    rec[content_start + 8..content_start + 16].copy_from_slice(&test_time.to_le_bytes());
    pos += si_attr_len as usize;

    // $FILE_NAME (0x30) — resident
    let name_bytes: Vec<u16> = name.encode_utf16().collect();
    let fn_content_size = 0x52u32 + (name_bytes.len() as u32) * 2;
    let fn_attr_len = 0x18 + fn_content_size;
    rec[pos..pos + 4].copy_from_slice(&0x30u32.to_le_bytes());
    rec[pos + 4..pos + 8].copy_from_slice(&fn_attr_len.to_le_bytes());
    rec[pos + 8] = 0; // resident
    rec[pos + 0x10..pos + 0x14].copy_from_slice(&fn_content_size.to_le_bytes());
    rec[pos + 0x14..pos + 0x16].copy_from_slice(&0x18u16.to_le_bytes());
    let fn_content = pos + 0x18;
    // parent_ref
    rec[fn_content..fn_content + 8].copy_from_slice(&parent.to_le_bytes());
    // timestamps
    rec[fn_content + 8..fn_content + 16].copy_from_slice(&test_time.to_le_bytes());
    rec[fn_content + 16..fn_content + 24].copy_from_slice(&test_time.to_le_bytes());
    rec[fn_content + 24..fn_content + 32].copy_from_slice(&test_time.to_le_bytes());
    rec[fn_content + 32..fn_content + 40].copy_from_slice(&test_time.to_le_bytes());
    rec[fn_content + 0x30..fn_content + 0x38].copy_from_slice(&1234u64.to_le_bytes());
    // name_len
    rec[fn_content + 0x40] = name_bytes.len() as u8;
    // name_namespace: 1 = Win32
    rec[fn_content + 0x41] = 1;
    // name (UTF-16LE)
    for (i, ch) in name_bytes.iter().enumerate() {
        let off = fn_content + 0x42 + i * 2;
        rec[off..off + 2].copy_from_slice(&ch.to_le_bytes());
    }

    // $DATA (0x80) — resident, size = 1234
    let data_attr_len = 0x18 + 0x08;
    let data_pos = pos + fn_attr_len as usize;
    if data_pos + data_attr_len <= rec.len() {
        rec[data_pos..data_pos + 4].copy_from_slice(&0x80u32.to_le_bytes());
        rec[data_pos + 4..data_pos + 8].copy_from_slice(&(data_attr_len as u32).to_le_bytes());
        rec[data_pos + 8] = 0; // resident
        rec[data_pos + 0x10..data_pos + 0x14].copy_from_slice(&1234u32.to_le_bytes());
        rec[data_pos + 0x14..data_pos + 0x16].copy_from_slice(&0x18u16.to_le_bytes());
    }

    rec
}

fn append_file_name_attr(rec: &mut [u8], name: &str, parent: u64, namespace: u8) {
    let mut pos = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
    while pos + 8 < rec.len() {
        let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
        if typ == 0xFFFFFFFF || typ == 0 {
            break;
        }
        let len = u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
        if len == 0 || pos + len > rec.len() {
            break;
        }
        pos += len;
    }

    let name_bytes: Vec<u16> = name.encode_utf16().collect();
    let fn_content_size = 0x52usize + name_bytes.len() * 2;
    let fn_attr_len = 0x18usize + fn_content_size;
    assert!(pos + fn_attr_len + 4 <= rec.len());
    rec[pos..pos + 4].copy_from_slice(&0x30u32.to_le_bytes());
    rec[pos + 4..pos + 8].copy_from_slice(&(fn_attr_len as u32).to_le_bytes());
    rec[pos + 8] = 0;
    rec[pos + 0x10..pos + 0x14].copy_from_slice(&(fn_content_size as u32).to_le_bytes());
    rec[pos + 0x14..pos + 0x16].copy_from_slice(&0x18u16.to_le_bytes());
    let content = pos + 0x18;
    rec[content..content + 8].copy_from_slice(&parent.to_le_bytes());
    rec[content + 0x40] = name_bytes.len() as u8;
    rec[content + 0x41] = namespace;
    for (index, ch) in name_bytes.iter().enumerate() {
        let off = content + 0x42 + index * 2;
        rec[off..off + 2].copy_from_slice(&ch.to_le_bytes());
    }
    let end = pos + fn_attr_len;
    rec[end..end + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
}

fn set_first_file_name_namespace(rec: &mut [u8], namespace: u8) {
    let mut pos = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
    while pos + 8 < rec.len() {
        let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
        let len = u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
        if typ == 0x30 {
            if let Some(content) = resident_content(rec, pos, len) {
                let namespace_offset = content.as_ptr() as usize - rec.as_ptr() as usize + 0x41;
                rec[namespace_offset] = namespace;
            }
            return;
        }
        if typ == 0xFFFFFFFF || len == 0 || pos + len > rec.len() {
            return;
        }
        pos += len;
    }
}

fn append_named_resident_data_attr(rec: &mut [u8], name: &str, size: u32) {
    let mut pos = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
    while pos + 8 < rec.len() {
        let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
        if typ == 0xFFFF_FFFF || typ == 0 {
            break;
        }
        let len = u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
        if len == 0 || pos + len > rec.len() {
            break;
        }
        pos += len;
    }

    let name_bytes: Vec<u16> = name.encode_utf16().collect();
    let name_bytes_len = name_bytes.len() * 2;
    let content_size = size as usize;
    let name_offset = 0x18usize;
    let content_offset = name_offset + name_bytes_len;
    let attr_len = content_offset + content_size;
    assert!(pos + attr_len + 4 <= rec.len());
    rec[pos..pos + 4].copy_from_slice(&0x80u32.to_le_bytes());
    rec[pos + 4..pos + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
    rec[pos + 8] = 0;
    rec[pos + 9] = name_bytes.len() as u8;
    rec[pos + 0x0a..pos + 0x0c].copy_from_slice(&(name_offset as u16).to_le_bytes());
    rec[pos + 0x10..pos + 0x14].copy_from_slice(&size.to_le_bytes());
    rec[pos + 0x14..pos + 0x16].copy_from_slice(&(content_offset as u16).to_le_bytes());
    for (index, ch) in name_bytes.iter().enumerate() {
        let off = pos + name_offset + index * 2;
        rec[off..off + 2].copy_from_slice(&ch.to_le_bytes());
    }
    let end = pos + attr_len;
    rec[end..end + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
}

fn remove_data_attrs(rec: &mut [u8]) {
    let mut pos = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
    while pos + 8 < rec.len() {
        let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
        if typ == 0xFFFF_FFFF || typ == 0 {
            break;
        }
        let len = u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
        if len == 0 || pos + len > rec.len() {
            break;
        }
        if typ == 0x80 {
            rec[pos..pos + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
            break;
        }
        pos += len;
    }
}

#[test]
fn parse_valid_file_record() {
    let mut parser = MftRecordParser::new(1024, 512);
    let rec = make_test_record(100, "test.txt", 5, false);
    let result = parser.parse(&rec, 100).unwrap();
    assert_eq!(result.name, "test.txt");
    assert_eq!(result.sequence_number, 7);
    assert_eq!(result.parent_ref, 5);
    assert!(!result.is_dir);
    assert!(!result.deleted);
    assert!(result.created_at.is_some());
    assert_eq!(result.size, 1234);
}

#[test]
fn parse_directory_record() {
    let mut parser = MftRecordParser::new(1024, 512);
    let rec = make_test_record(200, "Users", 5, true);
    let result = parser.parse(&rec, 200).unwrap();
    assert_eq!(result.name, "Users");
    assert!(result.is_dir);
}

#[test]
fn parse_multiple_file_name_attrs_keeps_selected_parent_ref() {
    let mut parser = MftRecordParser::new(1024, 512);
    let mut rec = make_test_record(300, "WINDOW~1", 5, true);
    set_first_file_name_namespace(&mut rec, 2);
    append_file_name_attr(&mut rec, "Windows", 42, 1);
    let result = parser.parse(&rec, 300).unwrap();
    assert_eq!(result.name, "Windows");
    assert_eq!(result.parent_ref, 42);
}

#[test]
fn named_data_stream_does_not_override_primary_file_size() {
    let mut parser = MftRecordParser::new(1024, 512);
    let mut rec = make_test_record(301, "System.evtx", 5, false);
    append_named_resident_data_attr(&mut rec, "Zone.Identifier", 0);
    let result = parser.parse(&rec, 301).unwrap();
    assert_eq!(result.size, 1234);
}

#[test]
fn file_name_real_size_is_fallback_when_data_attr_unavailable() {
    let mut parser = MftRecordParser::new(1024, 512);
    let mut rec = make_test_record(302, "SOFTWARE", 5, false);
    remove_data_attrs(&mut rec);
    let result = parser.parse(&rec, 302).unwrap();
    assert_eq!(result.size, 1234);
}

#[test]
fn parse_invalid_record() {
    let mut parser = MftRecordParser::new(1024, 512);
    let mut rec = vec![0u8; 1024];
    rec[0..4].copy_from_slice(b"BAAD");
    assert!(parser.parse(&rec, 0).is_none());
}

#[test]
fn parse_inactive_record() {
    let mut parser = MftRecordParser::new(1024, 512);
    let mut rec = make_test_record(500, "deleted.txt", 5, false);
    rec[0x16] = 0x00;
    let result = parser.parse(&rec, 500).unwrap();
    assert_eq!(result.name, "deleted.txt");
    assert!(result.deleted);
    assert_eq!(result.sequence_number, 7);
    assert!(!result.is_dir);
}

#[test]
fn parse_inactive_hidden_system_record() {
    let mut parser = MftRecordParser::new(1024, 512);
    let mut rec = make_test_record(501, "hidden-deleted.txt", 5, false);
    rec[0x16] = 0x00;
    let mut pos = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
    while pos + 8 < rec.len() {
        let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
        let len = u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
        if typ == 0x30 {
            if let Some(content) = resident_content(&rec, pos, len) {
                let flags_offset = content.as_ptr() as usize - rec.as_ptr() as usize + 0x38;
                rec[flags_offset..flags_offset + 4].copy_from_slice(&0x06u32.to_le_bytes());
            }
            break;
        }
        if typ == 0xFFFFFFFF || len == 0 || pos + len > rec.len() {
            break;
        }
        pos += len;
    }

    let result = parser.parse(&rec, 501).unwrap();
    assert!(result.deleted);
    assert!(result.hidden);
    assert!(result.system);
}

#[test]
fn scanner_parse_chunk() {
    let scanner = MftScanner::new(0, 0, 4096, 1024, 512, 1024 * 100);
    let mut buf = Vec::new();
    for _ in 0..10 {
        buf.extend_from_slice(&make_test_record(0, "file.txt", 5, false));
    }
    let records = scanner.parse_chunk(&buf, 0, 10);
    assert_eq!(records.len(), 10);
    assert_eq!(records[0].name, "file.txt");
}

#[test]
fn ntfs_time_conversion() {
    let ntfs_time: u64 = 127_111_680_000_000_000;
    let dt = ntfs_to_datetime(ntfs_time).unwrap();
    assert_eq!(dt.year(), 2003);
}

#[test]
fn zero_time_returns_none() {
    assert!(ntfs_to_datetime(0).is_none());
}

#[test]
fn active_and_inactive_records_keep_the_same_mft_sequence_identity() {
    let mut parser = MftRecordParser::new(1024, 512);
    let active = parser
        .parse(&make_test_record(600, "active.txt", 5, false), 600)
        .unwrap();
    let mut inactive_bytes = make_test_record(600, "deleted.txt", 5, false);
    inactive_bytes[0x16..0x18].copy_from_slice(&0u16.to_le_bytes());
    let inactive = parser.parse(&inactive_bytes, 600).unwrap();

    assert!(!active.deleted);
    assert!(inactive.deleted);
    assert_eq!(active.record_number, inactive.record_number);
    assert_eq!(active.sequence_number, inactive.sequence_number);
}
