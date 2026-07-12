use artifacts_linux::parse_journal;

fn build_synthetic_journal() -> Vec<u8> {
    let header_size: u64 = 240;
    let arena_size: u64 = 1024;

    let mut buf = Vec::with_capacity(arena_size as usize);
    buf.extend_from_slice(b"LPKSHHRH");
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.push(0u8);
    buf.extend_from_slice(&[0u8; 7]);
    buf.extend_from_slice(&[0u8; 16]);
    buf.extend_from_slice(&[0u8; 16]);
    buf.extend_from_slice(&[0xABu8; 16]);
    buf.extend_from_slice(&[0u8; 16]);
    buf.extend_from_slice(&header_size.to_le_bytes());
    buf.extend_from_slice(&arena_size.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&1u64.to_le_bytes());
    buf.extend_from_slice(&1u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());

    while buf.len() < header_size as usize {
        buf.push(0);
    }

    buf.extend_from_slice(&3u64.to_le_bytes());
    buf.extend_from_slice(&2u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&1u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());

    while buf.len() < 256 {
        buf.push(0);
    }

    while buf.len() % 8 != 0 {
        buf.push(0);
    }

    fn push_object_header(buf: &mut Vec<u8>, object_type: u8, payload_size: u64) {
        buf.push(object_type);
        buf.push(0);
        buf.extend_from_slice(&[0u8; 6]);
        buf.extend_from_slice(&payload_size.to_le_bytes());
    }

    push_object_header(&mut buf, 2, 16);
    buf.extend_from_slice(&10u64.to_le_bytes());
    buf.extend_from_slice(b"MESSAGE\0");
    while buf.len() % 8 != 0 {
        buf.push(0);
    }

    push_object_header(&mut buf, 2, 13);
    buf.extend_from_slice(&4u64.to_le_bytes());
    buf.extend_from_slice(b"_PID\0");
    while buf.len() % 8 != 0 {
        buf.push(0);
    }

    let data_msg_offset = buf.len() as u64;
    let message_text = b"Test journal message\n";
    push_object_header(&mut buf, 1, 8 + message_text.len() as u64);
    buf.extend_from_slice(&10u64.to_le_bytes());
    buf.extend_from_slice(message_text);
    while buf.len() % 8 != 0 {
        buf.push(0);
    }

    let data_pid_offset = buf.len() as u64;
    let pid_text = b"1234\n";
    push_object_header(&mut buf, 1, 8 + pid_text.len() as u64);
    buf.extend_from_slice(&4u64.to_le_bytes());
    buf.extend_from_slice(pid_text);
    while buf.len() % 8 != 0 {
        buf.push(0);
    }

    let data_ts_offset = buf.len() as u64;
    let ts_val: i64 = 1_700_000_000_000_000;
    let ts_text = format!("{}\n", ts_val);
    push_object_header(&mut buf, 1, 8 + ts_text.len() as u64);
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(ts_text.as_bytes());
    while buf.len() % 8 != 0 {
        buf.push(0);
    }

    let entry_offset = buf.len() as u64;
    push_object_header(&mut buf, 3, 3 * 16);
    buf.extend_from_slice(&data_ts_offset.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&data_msg_offset.to_le_bytes());
    buf.extend_from_slice(&10u64.to_le_bytes());
    buf.extend_from_slice(&data_pid_offset.to_le_bytes());
    buf.extend_from_slice(&4u64.to_le_bytes());
    while buf.len() % 8 != 0 {
        buf.push(0);
    }

    let entry_array_offset = buf.len() as u64;
    push_object_header(&mut buf, 6, 8);
    buf.extend_from_slice(&entry_offset.to_le_bytes());
    while buf.len() % 8 != 0 {
        buf.push(0);
    }

    buf[176..184].copy_from_slice(&entry_array_offset.to_le_bytes());
    buf[136..144].copy_from_slice(&entry_array_offset.to_le_bytes());

    buf
}

#[test]
fn parse_synthetic_journal_entries() {
    let data = build_synthetic_journal();
    let entries = parse_journal(&data).expect("should parse synthetic journal");
    assert!(!entries.is_empty(), "should find at least one entry");

    let entry = &entries[0];
    assert_eq!(entry.message.as_deref(), Some("Test journal message"));
    assert_eq!(entry.pid, Some(1234));
    assert!(entry.timestamp.is_some());
}

#[test]
fn reject_non_journal_data() {
    assert!(parse_journal(b"not a journal file").is_err());
}

#[test]
fn reject_empty_data() {
    assert!(parse_journal(&[]).is_err());
}

#[test]
fn reject_short_header() {
    let data = vec![0u8; 100];
    assert!(parse_journal(&data).is_err());
}
