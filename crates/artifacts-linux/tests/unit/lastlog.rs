use super::*;

fn build_slot_time32(ll_time: i32, line: &str, host: &str) -> Vec<u8> {
    let mut slot = vec![0u8; 292];
    slot[0..4].copy_from_slice(&ll_time.to_le_bytes());
    let line_bytes = line.as_bytes();
    slot[4..4 + line_bytes.len()].copy_from_slice(line_bytes);
    let host_bytes = host.as_bytes();
    slot[36..36 + host_bytes.len()].copy_from_slice(host_bytes);
    slot
}

fn build_slot_time64(ll_time: i64, line: &str, host: &str) -> Vec<u8> {
    let mut slot = vec![0u8; 296];
    slot[0..8].copy_from_slice(&ll_time.to_le_bytes());
    let line_bytes = line.as_bytes();
    slot[8..8 + line_bytes.len()].copy_from_slice(line_bytes);
    let host_bytes = host.as_bytes();
    slot[40..40 + host_bytes.len()].copy_from_slice(host_bytes);
    slot
}

#[test]
fn parse_lastlog_time32_sparse_multi_uid() {
    let login_ts = 1_700_000_000i32;
    let mut data = Vec::new();
    // UID 0 (root) logged in from a local console.
    data.extend(build_slot_time32(login_ts, "tty1", ""));
    // UIDs 1..1000 are all-zero holes (never logged in).
    data.extend(std::iter::repeat_n(0u8, 292 * 999));
    // UID 1000 logged in over SSH.
    data.extend(build_slot_time32(login_ts + 3600, "pts/0", "192.168.1.100"));

    let records = parse_lastlog(&data).expect("should parse sparse lastlog");
    assert_eq!(records.len(), 2, "all-zero UID slots must be skipped");

    assert_eq!(records[0].uid, 0);
    assert_eq!(records[0].line, "tty1");
    assert_eq!(records[0].host, "");
    assert_eq!(
        records[0].time.map(|ts| ts.timestamp()),
        Some(i64::from(login_ts))
    );

    assert_eq!(records[1].uid, 1000);
    assert_eq!(records[1].line, "pts/0");
    assert_eq!(records[1].host, "192.168.1.100");
    assert_eq!(
        records[1].time.map(|ts| ts.timestamp()),
        Some(i64::from(login_ts) + 3600)
    );
}

#[test]
fn parse_lastlog_time64_layout() {
    let login_ts = 1_700_000_000i64;
    let mut data = Vec::new();
    data.extend(build_slot_time64(login_ts, "pts/3", "10.0.0.5"));
    data.extend(build_slot_time64(0, "", "")); // all-zero hole at UID 1
                                               // UID 2 carries line content but a zero ("never logged in") timestamp.
    data.extend(build_slot_time64(0, "pts/4", ""));

    let records = parse_lastlog(&data).expect("should parse 296-byte lastlog");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].uid, 0);
    assert_eq!(records[0].host, "10.0.0.5");
    assert_eq!(
        records[0].time.map(|ts| ts.timestamp()),
        Some(login_ts),
        "64-bit ll_time must be read at full width"
    );
    // Content but zero timestamp: kept with no time.
    assert_eq!(records[1].uid, 2);
    assert_eq!(records[1].line, "pts/4");
    assert_eq!(records[1].time, None);
}

#[test]
fn parse_lastlog_empty_file_yields_no_records() {
    let records = parse_lastlog(&[]).expect("empty lastlog is valid");
    assert!(records.is_empty());
}

#[test]
fn parse_lastlog_tolerates_truncated_tail() {
    let mut data = build_slot_time32(1_700_000_000, "tty2", "");
    data.extend_from_slice(&[0xAA; 100]); // partial trailing record

    let records = parse_lastlog(&data).expect("truncated tail is tolerated");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].uid, 0);
    assert_eq!(records[0].line, "tty2");
}

#[test]
fn parse_lastlog_rejects_garbage() {
    let data = vec![0xFFu8; 292];
    let error = parse_lastlog(&data).expect_err("non-printable slot content must fail");
    assert!(error.to_string().contains("lastlog"));
}
