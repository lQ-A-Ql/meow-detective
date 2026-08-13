//! Integration tests for the artifacts-linux crate.
//!
//! Tests all 6 Linux artifact parsers with synthetic fixtures,
//! verifies graceful empty-input handling, and cross-parser
//! timestamp compatibility.

use artifacts_linux::{
    parse_apt_history, parse_auth_log_sudo, parse_bash_history, parse_crontab, parse_dpkg_log,
    parse_journal, parse_wtmp, LogTimeHint, SudoEvent, UtcClock,
};
use chrono::{TimeZone, Utc};

// ─────────────────────────────────────────────────────────────────────────────
// 1. All 6 parsers accept their synthetic fixtures
// ─────────────────────────────────────────────────────────────────────────────

// ── apt: parse_apt_history ──────────────────────────────────────────────────

#[test]
fn apt_history_parses_complete_fixture() {
    let input = "\
Start-Date: 2024-01-15  10:30:00
Commandline: apt-get install curl vim
Install: curl:amd64 (7.88.1-10+deb12u5), vim:amd64 (2:9.0.1378-2)
End-Date: 2024-01-15  10:30:15

Start-Date: 2024-06-01  14:00:00
Upgrade: libssl3:amd64 (3.0.11-1~deb12u2), openssl:amd64 (3.0.11-1~deb12u2)
Remove: old-lib:amd64 (1.2.3-4)
End-Date: 2024-06-01  14:00:05";

    let events = parse_apt_history(input, &UtcClock).expect("should parse APT history");
    assert_eq!(
        events.len(),
        5,
        "expected 5 events (2 install + 2 upgrade + 1 remove)"
    );

    assert_eq!(events[0].action, "install");
    assert_eq!(events[0].package, "curl:amd64");
    assert_eq!(events[0].version.as_deref(), Some("7.88.1-10+deb12u5"));
    assert!(events[0].timestamp.is_some());

    assert_eq!(events[4].action, "remove");
    assert_eq!(events[4].package, "old-lib:amd64");
}

#[test]
fn dpkg_log_parses_complete_fixture() {
    let input = "\
2024-01-15 10:30:00 install curl:amd64 7.88.1-10+deb12u5
2024-01-15 10:30:01 configure curl:amd64 7.88.1-10+deb12u5 <none>
2024-06-01 14:00:00 upgrade libssl3:amd64 3.0.11-1~deb12u2 <none>
2024-06-02 09:15:03 remove old-package:amd64 1.0.0-1 <none>";

    let events = parse_dpkg_log(input, &UtcClock).expect("should parse dpkg log");
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].action, "install");
    assert_eq!(events[0].package, "curl:amd64");
    assert_eq!(events[0].version.as_deref(), Some("7.88.1-10+deb12u5"));
    assert!(events[0].timestamp.is_some());
}

// ── bash: parse_bash_history ────────────────────────────────────────────────

#[test]
fn bash_history_parses_complete_fixture() {
    let input = "\
#1705276800
ls -la /home
#1705276810
cat /etc/hostname
echo hello world
# This is a comment
#1705276820
git status";

    let cmds = parse_bash_history(input).expect("should parse bash history");
    assert_eq!(cmds.len(), 4);

    assert_eq!(cmds[0].command, "ls -la /home");
    assert!(cmds[0].timestamp.is_some());
    assert_eq!(cmds[0].timestamp.unwrap().timestamp(), 1705276800);

    assert_eq!(cmds[1].command, "cat /etc/hostname");
    assert!(cmds[1].timestamp.is_some());

    assert_eq!(cmds[2].command, "echo hello world");
    assert!(
        cmds[2].timestamp.is_none(),
        "no timestamp line before this command"
    );

    assert_eq!(cmds[3].command, "git status");
    assert!(cmds[3].timestamp.is_some());
}

// ── cron: parse_crontab ─────────────────────────────────────────────────────

#[test]
fn crontab_parses_complete_fixture() {
    let input = "\
# Edit this file to introduce tasks
SHELL=/bin/bash
PATH=/usr/local/sbin:/usr/local/bin:/sbin:/bin:/usr/sbin:/usr/bin

# m h  dom mon dow   command
30 2 * * * /usr/bin/backup.sh
0 */6 * * * /usr/local/bin/cleanup.sh
@daily /usr/bin/logrotate
@reboot root /usr/local/bin/startup.sh
0 0 * * * root /usr/bin/system-maintenance";

    let jobs = parse_crontab(input).expect("should parse crontab");
    assert!(
        jobs.len() >= 5,
        "expected at least 5 cron jobs, got {}",
        jobs.len()
    );

    let backup = jobs
        .iter()
        .find(|j| j.command == "/usr/bin/backup.sh")
        .unwrap();
    assert_eq!(backup.schedule, "30 2 * * *");
    assert_eq!(backup.source_file, "<unknown>");

    let daily = jobs.iter().find(|j| j.schedule == "@daily").unwrap();
    assert_eq!(daily.command, "/usr/bin/logrotate");

    let reboot = jobs.iter().find(|j| j.schedule == "@reboot").unwrap();
    assert_eq!(reboot.user.as_deref(), Some("root"));
}

// ── journal: parse_journal ──────────────────────────────────────────────────

mod common;

/// The synthetic journal fixture lives in `tests/common/mod.rs` and follows
/// the documented on-disk format (48-byte ENTRY header, 48-byte DATA header,
/// ENTRY_ARRAY chain, real lookup3/SipHash payload hashes).
#[test]
fn journal_parses_synthetic_fixture() {
    let data = common::build_journal(&common::base_spec());
    let entries = parse_journal(&data).expect("should parse synthetic journal");
    assert_eq!(entries.len(), 2, "should find both entries");

    let entry = &entries[0];
    assert_eq!(entry.message.as_deref(), Some("Test journal message"));
    assert_eq!(entry.pid, Some(1234));
    assert!(entry.timestamp.is_some());
}

// ── sudo: parse_auth_log_sudo ───────────────────────────────────────────────

#[test]
fn sudo_auth_log_parses_complete_fixture() {
    let input = "\
Jan 15 10:30:00 ubuntu sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/apt update
Jan 15 10:30:05 ubuntu sudo: pam_unix(sudo:session): session opened for user root by alice(uid=0)
Jan 15 10:32:00 ubuntu sudo:     bob : TTY=pts/1 ; PWD=/home/bob ; USER=root ; COMMAND=/usr/bin/systemctl restart nginx
Jan 15 10:32:05 ubuntu sudo: pam_unix(sudo:session): session opened for user root by bob(uid=0)";

    let events =
        parse_auth_log_sudo(input, &LogTimeHint::utc(None)).expect("should parse auth log");
    let cmds: Vec<&SudoEvent> = events
        .iter()
        .filter(|e| !e.command.contains("authentication failure"))
        .collect();
    assert_eq!(cmds.len(), 2);

    assert_eq!(cmds[0].user, "alice");
    assert_eq!(cmds[0].command, "/usr/bin/apt update");
    assert_eq!(cmds[0].target_user.as_deref(), Some("root"));
    assert_eq!(cmds[0].terminal.as_deref(), Some("pts/0"));

    assert_eq!(cmds[1].user, "bob");
    assert_eq!(cmds[1].command, "/usr/bin/systemctl restart nginx");
}

// ── wtmp: parse_wtmp ────────────────────────────────────────────────────────

const WTMP_SIZE_64: usize = 400;
const USER_PROCESS: i32 = 7;
const DEAD_PROCESS: i32 = 8;
const BOOT_TIME: i32 = 2;

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
fn wtmp_parses_login_logout_fixture() {
    let login_ts: i64 = 1_700_000_000;
    let logout_ts: i64 = 1_700_010_000;

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
    assert_eq!(records[0].pid, 12345);
    assert!(records[0].login_time.is_some());
    assert!(records[0].logout_time.is_some());
    assert_eq!(records[0].login_time.unwrap().timestamp(), login_ts);
    assert_eq!(records[0].logout_time.unwrap().timestamp(), logout_ts);
}

#[test]
fn wtmp_parses_boot_record() {
    let boot_ts: i64 = 1_700_000_000;
    let data = build_wtmp_record_64(BOOT_TIME, 0, "reboot", "~", "", boot_ts, 0);

    let records = parse_wtmp(&data).expect("should parse wtmp");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].user, "reboot");
    assert_eq!(records[0].record_type, BOOT_TIME);
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Graceful handling of empty input for each parser
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn apt_empty_input() {
    assert!(parse_apt_history("", &UtcClock).unwrap().is_empty());
    assert!(parse_dpkg_log("", &UtcClock).unwrap().is_empty());
}

#[test]
fn bash_empty_input() {
    assert!(parse_bash_history("").unwrap().is_empty());
}

#[test]
fn cron_empty_input() {
    assert!(parse_crontab("").unwrap().is_empty());
}

#[test]
fn sudo_empty_input() {
    assert!(parse_auth_log_sudo("", &LogTimeHint::utc(None))
        .unwrap()
        .is_empty());
}

#[test]
fn journal_rejects_empty_input() {
    assert!(parse_journal(&[]).is_err());
}

#[test]
fn journal_rejects_short_data() {
    assert!(parse_journal(&[0u8; 100]).is_err());
}

#[test]
fn wtmp_rejects_empty_input() {
    assert!(parse_wtmp(&[]).is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Cross-parser: wtmp + bash produce compatible timestamps
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn wtmp_and_bash_timestamps_are_chrono_compatible() {
    let ts: i64 = 1_705_276_800; // 2024-01-15T00:00:00 UTC

    // wtmp timestamp
    let data = build_wtmp_record_64(USER_PROCESS, 1111, "testuser", "pts/0", "", ts, 0);
    let records = parse_wtmp(&data).expect("should parse wtmp");
    let wt_ts = records[0].login_time.unwrap();

    // bash history timestamp from the same epoch
    let bash_input = format!("#{}\nls", ts);
    let cmds = parse_bash_history(&bash_input).expect("should parse bash history");
    let bash_ts = cmds[0].timestamp.unwrap();

    // Both should resolve to the same chrono DateTime
    assert_eq!(wt_ts.timestamp(), bash_ts.timestamp());
    assert_eq!(wt_ts.timestamp(), ts);

    // Additional sanity: both are well-formed datetimes
    let expected = Utc.timestamp_opt(ts, 0).single().unwrap();
    assert_eq!(wt_ts, expected);
    assert_eq!(bash_ts, expected);

    // Round-trip formatting
    assert_eq!(wt_ts.to_rfc3339(), expected.to_rfc3339());
    assert_eq!(bash_ts.to_rfc3339(), expected.to_rfc3339());
}

#[test]
fn wtmp_and_bash_different_timestamps_are_correctly_ordered() {
    let earlier = 1_700_000_000;
    let later = 1_705_000_000;

    // wtmp records
    let mut data = Vec::new();
    data.extend(build_wtmp_record_64(
        USER_PROCESS,
        1,
        "a",
        "pts/0",
        "",
        earlier,
        0,
    ));
    data.extend(build_wtmp_record_64(
        USER_PROCESS,
        2,
        "b",
        "pts/1",
        "",
        later,
        0,
    ));
    let records = parse_wtmp(&data).expect("should parse wtmp");
    assert!(records[0].login_time.unwrap() < records[1].login_time.unwrap());

    // bash history
    let bash_input = format!("#{}\ncmd1\n#{}\ncmd2", earlier, later);
    let cmds = parse_bash_history(&bash_input).expect("should parse bash history");
    assert!(cmds[0].timestamp.unwrap() < cmds[1].timestamp.unwrap());
}
