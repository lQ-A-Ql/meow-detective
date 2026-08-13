use super::*;

fn build_slot_lp64(
    fail_cnt: i16,
    fail_max: i16,
    line: &str,
    fail_time: i64,
    locktime: i64,
) -> Vec<u8> {
    let mut slot = vec![0u8; 32];
    slot[0..2].copy_from_slice(&fail_cnt.to_le_bytes());
    slot[2..4].copy_from_slice(&fail_max.to_le_bytes());
    let line_bytes = line.as_bytes();
    slot[4..4 + line_bytes.len()].copy_from_slice(line_bytes);
    slot[16..24].copy_from_slice(&fail_time.to_le_bytes());
    slot[24..32].copy_from_slice(&locktime.to_le_bytes());
    slot
}

fn build_slot_ilp32(
    fail_cnt: i16,
    fail_max: i16,
    line: &str,
    fail_time: i32,
    locktime: i32,
) -> Vec<u8> {
    let mut slot = vec![0u8; 24];
    slot[0..2].copy_from_slice(&fail_cnt.to_le_bytes());
    slot[2..4].copy_from_slice(&fail_max.to_le_bytes());
    let line_bytes = line.as_bytes();
    slot[4..4 + line_bytes.len()].copy_from_slice(line_bytes);
    slot[16..20].copy_from_slice(&fail_time.to_le_bytes());
    slot[20..24].copy_from_slice(&locktime.to_le_bytes());
    slot
}

#[test]
fn parse_faillog_lp64_sparse_multi_uid() {
    let fail_ts = 1_700_000_000i64;
    let mut data = Vec::new();
    // UID 0 (root): 3 failures, threshold 5, 15-minute lock window.
    data.extend(build_slot_lp64(3, 5, "pts/0", fail_ts, 900));
    // UIDs 1..100 are all-zero holes.
    data.extend(std::iter::repeat_n(0u8, 32 * 99));
    // UID 100: failures reached the lockout threshold.
    data.extend(build_slot_lp64(5, 5, "pts/2", fail_ts + 60, 0));

    let records = parse_faillog(&data).expect("should parse sparse faillog");
    assert_eq!(records.len(), 2, "all-zero UID slots must be skipped");

    assert_eq!(records[0].uid, 0);
    assert_eq!(records[0].failure_count, 3);
    assert_eq!(records[0].max_failures, 5);
    assert_eq!(records[0].line, "pts/0");
    assert_eq!(
        records[0].last_failure.map(|ts| ts.timestamp()),
        Some(fail_ts)
    );
    assert_eq!(records[0].locktime_seconds, 900);
    assert!(!records[0].lockout, "3 < 5 is below the lockout threshold");

    assert_eq!(records[1].uid, 100);
    assert!(records[1].lockout, "fail_cnt == fail_max means lockout");
    assert_eq!(
        records[1].last_failure.map(|ts| ts.timestamp()),
        Some(fail_ts + 60)
    );
}

#[test]
fn parse_faillog_ilp32_layout() {
    let fail_ts = 1_600_000_000i32;
    let mut data = Vec::new();
    data.extend(build_slot_ilp32(1, 0, "tty1", fail_ts, 0));
    data.extend(build_slot_ilp32(0, 0, "", 0, 0)); // all-zero hole at UID 1
    data.extend(build_slot_ilp32(2, 3, "pts/5", fail_ts + 30, 120));

    let records = parse_faillog(&data).expect("should parse 24-byte faillog");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].uid, 0);
    assert_eq!(records[0].line, "tty1");
    assert_eq!(
        records[0].last_failure.map(|ts| ts.timestamp()),
        Some(i64::from(fail_ts)),
        "32-bit fail_time must be read at 4-byte width"
    );
    assert_eq!(records[1].uid, 2);
    assert_eq!(records[1].locktime_seconds, 120);
    assert!(!records[1].lockout, "2 < 3 is below the lockout threshold");
}

#[test]
fn parse_faillog_empty_file_yields_no_records() {
    let records = parse_faillog(&[]).expect("empty faillog is valid");
    assert!(records.is_empty());
}

#[test]
fn parse_faillog_tolerates_truncated_tail() {
    let mut data = build_slot_lp64(1, 3, "pts/1", 1_700_000_000, 60);
    data.extend_from_slice(&[0xAA; 7]); // partial trailing record

    let records = parse_faillog(&data).expect("truncated tail is tolerated");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].uid, 0);
    assert_eq!(records[0].line, "pts/1");
}

#[test]
fn parse_faillog_rejects_garbage() {
    let data = vec![0xFFu8; 32];
    let error = parse_faillog(&data).expect_err("non-printable slot content must fail");
    assert!(error.to_string().contains("faillog"));
}
