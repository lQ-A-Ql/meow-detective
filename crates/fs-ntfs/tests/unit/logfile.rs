use super::*;

/// Build a minimal synthetic $LogFile for testing.
///
/// The returned buffer simulates:
///   - 2 RSTR pages (pages 0-1) — each with one restart area
///   - 1 RCRD page (page 2) — with the supplied log records
fn build_synthetic_logfile(records: &[(u64, u16, u16, &[u8])]) -> Vec<u8> {
    let num_pages = 3usize; // 2 RSTR + 1 RCRD
    let mut data = vec![0u8; num_pages * LOG_PAGE_SIZE];

    // --- RSTR page 0 (restart area 1) ---
    let rstr0 = &mut data[0..LOG_PAGE_SIZE];
    rstr0[0..4].copy_from_slice(&RSTR_MAGIC.to_le_bytes());
    rstr0[4..8].copy_from_slice(&0u32.to_le_bytes()); // usa_offset, usa_count placeholder
    rstr0[0x0E..0x10].copy_from_slice(&1u16.to_le_bytes()); // valid flag=1
    rstr0[0x18..0x20].copy_from_slice(&1u64.to_le_bytes()); // client count
    rstr0[0x20..0x24].copy_from_slice(&1u32.to_le_bytes()); // client index
    rstr0[0x24..0x28].copy_from_slice(&2u32.to_le_bytes()); // client type

    // --- RSTR page 1 (restart area 2, backup copy) ---
    let rstr1 = &mut data[LOG_PAGE_SIZE..2 * LOG_PAGE_SIZE];
    rstr1[0..4].copy_from_slice(&RSTR_MAGIC.to_le_bytes());
    rstr1[4..8].copy_from_slice(&0u32.to_le_bytes());
    rstr1[0x0E..0x10].copy_from_slice(&1u16.to_le_bytes()); // valid flag=2
    rstr1[0x18..0x20].copy_from_slice(&1u64.to_le_bytes());
    rstr1[0x20..0x24].copy_from_slice(&1u32.to_le_bytes());
    rstr1[0x24..0x28].copy_from_slice(&2u32.to_le_bytes());

    // --- RCRD page (page 2) ---
    let rcrd_page = &mut data[2 * LOG_PAGE_SIZE..];
    rcrd_page[0..4].copy_from_slice(&RCRD_MAGIC.to_le_bytes());
    rcrd_page[0x08..0x10].copy_from_slice(&(records.len() as u64).to_le_bytes()); // last_lsn
    rcrd_page[0x18..0x1A].copy_from_slice(&(records.len() as u16).to_le_bytes()); // page_count
    rcrd_page[0x1C..0x1E].copy_from_slice(&0x40u16.to_le_bytes()); // next_record_offset

    // Write each record
    let mut rec_off = 0x40usize; // start after the page header area
    for &(_file_ref, _operation, target_attr, extra) in records {
        let header_size = 0x48usize;
        let client_data_len = extra.len();
        let record_size = (header_size + client_data_len + 7) & !7;

        if rec_off + record_size > LOG_PAGE_SIZE {
            break;
        }

        let rec = &mut rcrd_page[rec_off..rec_off + record_size];

        // LSN (just use growing counter)
        rec[0..8].copy_from_slice(&(rec_off as u64).to_le_bytes());
        rec[0x18..0x1C].copy_from_slice(&(client_data_len as u32).to_le_bytes()); // client_data_length
        rec[0x20..0x24].copy_from_slice(&0u32.to_le_bytes()); // record_type = normal
        rec[0x28..0x2A].copy_from_slice(&1u16.to_le_bytes()); // flags

        // Redo operation at offset 0x2C..0x30
        rec[0x2C..0x2E].copy_from_slice(&(header_size as u16).to_le_bytes()); // redo_off
        rec[0x30..0x32].copy_from_slice(&(client_data_len as u16).to_le_bytes()); // redo_len

        // Undo: nothing (offset = 0, length = 0)
        rec[0x32..0x34].copy_from_slice(&0u16.to_le_bytes()); // undo_len

        rec[0x34..0x36].copy_from_slice(&target_attr.to_le_bytes()); // target_attribute

        // Copy the client data
        let client_start = header_size;
        rec[client_start..client_start + client_data_len].copy_from_slice(extra);

        rec_off += record_size;
    }

    data
}

/// Build the bytes for a synthetic redo/undo update record.
/// Layout:
///   +0x00: file_ref (u64)
///   +0x08: operation (u16)
///   +0x0A: reserved (6 bytes of zeros)
///   +0x10: timestamp (u64)
fn build_update_record(file_ref: u64, operation: u16, timestamp: u64) -> Vec<u8> {
    let mut data = vec![0u8; 24]; // 16-byte header + 8-byte timestamp
    data[0..8].copy_from_slice(&file_ref.to_le_bytes());
    data[8..10].copy_from_slice(&operation.to_le_bytes());
    // reserved bytes 0x0A..0x0F remain zero
    data[0x10..0x18].copy_from_slice(&timestamp.to_le_bytes());
    data
}

#[test]
fn empty_logfile_returns_no_changes() {
    let changes = build_file_change_history(&[]);
    assert!(changes.is_empty());
}

#[test]
fn logfile_with_zero_pages_returns_no_changes() {
    // Fewer than one page
    let data = vec![0u8; 100];
    let changes = build_file_change_history(&data);
    assert!(changes.is_empty());
}

#[test]
fn single_create_record_extracted() {
    let update_data = build_update_record(42, 0x0001, 0);
    let log_data = build_synthetic_logfile(&[(42, 0x0001, 0x80, &update_data)]);

    let changes = build_file_change_history(&log_data);
    assert!(
        !changes.is_empty(),
        "expected at least 1 change, got {}",
        changes.len()
    );

    let create = changes.iter().find(|c| c.operation == "create");
    assert!(create.is_some(), "expected a 'create' change");
    if let Some(c) = create {
        assert_eq!(c.file_ref, 42);
        assert_eq!(c.attribute, "$DATA");
    }
}

#[test]
fn multiple_records_extracted() {
    let rec1 = build_update_record(10, 0x0001, 100); // create file 10
    let rec2 = build_update_record(11, 0x0002, 200); // delete file 11
    let rec3 = build_update_record(12, 0x0008, 300); // write file 12

    let log_data = build_synthetic_logfile(&[
        (10, 0x0001, 0x80, &rec1),
        (11, 0x0002, 0x30, &rec2),
        (12, 0x0008, 0x80, &rec3),
    ]);

    let changes = build_file_change_history(&log_data);
    assert!(
        changes.len() >= 3,
        "expected at least 3 changes, got {}",
        changes.len()
    );

    let ops: Vec<&str> = changes.iter().map(|c| c.operation.as_str()).collect();
    assert!(ops.contains(&"create"), "missing 'create'");
    assert!(ops.contains(&"delete"), "missing 'delete'");
    assert!(ops.contains(&"write"), "missing 'write'");
}

#[test]
fn attribute_type_code_mapped_correctly() {
    assert_eq!(attribute_type_name(0x10), "$STANDARD_INFORMATION");
    assert_eq!(attribute_type_name(0x30), "$FILE_NAME");
    assert_eq!(attribute_type_name(0x80), "$DATA");
    assert_eq!(attribute_type_name(0x90), "$INDEX_ROOT");
    assert_eq!(attribute_type_name(0xA0), "$INDEX_ALLOCATION");
    assert_eq!(attribute_type_name(0xFFFF), "UNKNOWN");
}

#[test]
fn rstr_page_skipped_gracefully() {
    // Just an RSTR page (no RCRD) — should produce no changes
    let mut data = vec![0u8; LOG_PAGE_SIZE];
    data[0..4].copy_from_slice(&RSTR_MAGIC.to_le_bytes());

    let changes = build_file_change_history(&data);
    assert!(changes.is_empty());
}

#[test]
fn corrupt_rcrd_page_does_not_panic() {
    // An RCRD page with garbage content — must not panic
    let mut data = vec![0u8; LOG_PAGE_SIZE * 2];
    data[LOG_PAGE_SIZE..LOG_PAGE_SIZE + 4].copy_from_slice(&RCRD_MAGIC.to_le_bytes());
    // Fill with random values but let next_record_offset point to valid range
    data[LOG_PAGE_SIZE + 0x1C..LOG_PAGE_SIZE + 0x1E].copy_from_slice(&0x40u16.to_le_bytes());
    // Garbage content after header
    for i in (LOG_PAGE_SIZE + 0x40..LOG_PAGE_SIZE + LOG_PAGE_SIZE).step_by(4) {
        data[i..i + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    }

    let changes = build_file_change_history(&data);
    // May produce zero or some changes, but must not panic.
    let _ = changes;
}

#[test]
fn timestamp_extracted_from_records() {
    let ts: u64 = 132744456000000000; // 2021-09-01 in NTFS time
    let update_data = build_update_record(99, 0x0006, ts);
    let log_data = build_synthetic_logfile(&[(99, 0x0006, 0x10, &update_data)]);

    let changes = build_file_change_history(&log_data);
    let set_info = changes.iter().find(|c| c.operation == "set_info");
    assert!(set_info.is_some(), "expected 'set_info' operation");
    if let Some(c) = set_info {
        assert_eq!(c.timestamp, ts);
        assert_eq!(c.file_ref, 99);
    }
}
