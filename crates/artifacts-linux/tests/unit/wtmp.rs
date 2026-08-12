use super::*;

fn build_wtmp_record_64(
    ut_type: i32,
    ut_pid: i32,
    user: &str,
    line: &str,
    host: &str,
    tv_sec: i64,
    tv_usec: i64,
) -> Vec<u8> {
    let mut buf = vec![0u8; WTMP_SIZE_64];
    buf[0..4].copy_from_slice(&ut_type.to_le_bytes());
    buf[4..8].copy_from_slice(&ut_pid.to_le_bytes());
    let line_bytes = line.as_bytes();
    let copy_len = line_bytes.len().min(32);
    buf[8..8 + copy_len].copy_from_slice(&line_bytes[..copy_len]);
    let user_bytes = user.as_bytes();
    let copy_len = user_bytes.len().min(32);
    buf[44..44 + copy_len].copy_from_slice(&user_bytes[..copy_len]);
    let host_bytes = host.as_bytes();
    let copy_len = host_bytes.len().min(256);
    buf[76..76 + copy_len].copy_from_slice(&host_bytes[..copy_len]);
    buf[344..352].copy_from_slice(&tv_sec.to_le_bytes());
    buf[352..360].copy_from_slice(&tv_usec.to_le_bytes());
    buf
}

#[test]
fn parse_wtmp_64_login_logout() {
    let login_ts = 1_700_000_000;
    let logout_ts = 1_700_010_000;
    let mut data = Vec::new();
    data.extend(build_wtmp_record_64(
        USER_PROCESS,
        12345,
        "alice",
        "pts/0",
        "192.168.1.100",
        login_ts,
        0,
    ));
    data.extend(build_wtmp_record_64(
        DEAD_PROCESS,
        12345,
        "",
        "pts/0",
        "",
        logout_ts,
        0,
    ));
    let records = parse_wtmp(&data).expect("should parse wtmp");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].user, "alice");
    assert_eq!(records[0].terminal, "pts/0");
    assert_eq!(records[0].host, "192.168.1.100");
    assert_eq!(records[0].pid, 12345);
    assert_eq!(records[0].login_time.unwrap().timestamp(), login_ts);
    assert_eq!(records[0].logout_time.unwrap().timestamp(), logout_ts);
}

#[test]
fn parse_wtmp_boot_record() {
    let boot_ts = 1_700_000_000;
    let data = build_wtmp_record_64(BOOT_TIME, 0, "reboot", "~", "", boot_ts, 0);
    let records = parse_wtmp(&data).expect("should parse wtmp");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].user, "reboot");
    assert_eq!(records[0].record_type, BOOT_TIME);
}

#[test]
fn parse_wtmp_32bit_layout() {
    let mut buf = vec![0u8; WTMP_SIZE_32];
    buf[0..4].copy_from_slice(&USER_PROCESS.to_le_bytes());
    buf[4..8].copy_from_slice(&9999i32.to_le_bytes());
    buf[44..48].copy_from_slice(b"bob\0");
    buf[8..12].copy_from_slice(b"tty2");
    let login_ts = 1_700_000_000i64;
    buf[340..344].copy_from_slice(&(login_ts as i32).to_le_bytes());
    buf[344..348].copy_from_slice(&0i32.to_le_bytes());
    let records = parse_wtmp(&buf).expect("should parse 32-bit wtmp");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].user, "bob");
}

#[test]
fn reject_empty_data() {
    assert!(parse_wtmp(&[]).is_err());
}

#[test]
fn parse_wtmp_32bit_timestamp_with_usec() {
    // 32-bit layout stores tv_sec/tv_usec as separate 4-byte fields; a
    // non-zero usec must not bleed into tv_sec's high bytes.
    let mut buf = vec![0u8; WTMP_SIZE_32];
    buf[0..4].copy_from_slice(&USER_PROCESS.to_le_bytes());
    buf[4..8].copy_from_slice(&4242i32.to_le_bytes());
    buf[8..13].copy_from_slice(b"pts/3");
    buf[44..49].copy_from_slice(b"carol");
    let login_ts = 1_700_000_000i32;
    buf[340..344].copy_from_slice(&login_ts.to_le_bytes());
    buf[344..348].copy_from_slice(&500_000i32.to_le_bytes());
    let records = parse_wtmp(&buf).expect("should parse 32-bit wtmp");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].user, "carol");
    assert_eq!(
        records[0].login_time.expect("login timestamp").timestamp(),
        1_700_000_000
    );
}

#[test]
fn reject_non_wtmp_content() {
    // 1000 bytes of ASCII text is >= one 400-byte record but must not be
    // accepted as wtmp just because of its length.
    let data = b"the quick brown fox jumps over the lazy dog\n".repeat(25);
    assert!(parse_wtmp(&data).is_err());
}

#[test]
fn tolerate_truncated_trailing_record() {
    let mut data = build_wtmp_record_64(USER_PROCESS, 777, "dave", "pts/4", "", 1_700_000_000, 0);
    data.extend_from_slice(&[0xAAu8; 150]); // partial trailing record
    let records = parse_wtmp(&data).expect("should parse with truncated tail");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].user, "dave");
    assert_eq!(
        records[0].login_time.expect("login timestamp").timestamp(),
        1_700_000_000
    );
}

#[test]
fn runlevel_record_unpacks_packed_pid() {
    // RUN_LVL packs the current runlevel char in the low byte of ut_pid and
    // the previous runlevel in the second byte (e.g. 0x5335 = '5'/'S').
    let packed = ((b'S' as i32) << 8) | b'5' as i32;
    let data = build_wtmp_record_64(RUN_LVL, packed, "runlevel", "~", "", 1_700_000_000, 0);
    let records = parse_wtmp(&data).expect("should parse runlevel record");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].user, "runlevel-5");
}
