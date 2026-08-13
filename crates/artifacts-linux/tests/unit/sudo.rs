use super::*;
use crate::clock::{LogClock, LogTimeHint, UtcClock};
use chrono::{FixedOffset, TimeZone};

fn reference(y: i32, m: u32, d: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
}

#[test]
fn parse_sudo_command_lines() {
    let input = "\
Jan 15 10:30:00 ubuntu sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/apt update
Jan 15 10:30:05 ubuntu sudo: pam_unix(sudo:session): session opened for user root by alice(uid=0)
Jan 15 10:32:00 ubuntu sudo:     bob : TTY=pts/1 ; PWD=/home/bob ; USER=root ; COMMAND=/usr/bin/systemctl restart nginx
Jan 15 10:32:05 ubuntu sudo: pam_unix(sudo:session): session opened for user root by bob(uid=0)
Jan 15 10:35:00 ubuntu sudo: pam_unix(sudo:session): session closed for user root";
    let events =
        parse_auth_log_sudo(input, &LogTimeHint::utc(None)).expect("should parse auth log");
    let cmds: Vec<&SudoEvent> = events
        .iter()
        .filter(|e| !e.command.contains("authentication failure"))
        .collect();
    assert_eq!(cmds.len(), 2);
    assert_eq!(cmds[0].user, "alice");
    assert_eq!(cmds[0].command, "/usr/bin/apt update");
    assert_eq!(cmds[0].working_directory.as_deref(), Some("/home/alice"));
    assert_eq!(cmds[0].target_user.as_deref(), Some("root"));
    assert_eq!(cmds[0].terminal.as_deref(), Some("pts/0"));
    assert!(cmds[0].success);
    assert_eq!(cmds[1].user, "bob");
    assert_eq!(cmds[1].command, "/usr/bin/systemctl restart nginx");
    assert!(cmds[1].success);
}

#[test]
fn command_before_session_open_is_successful() {
    let input = "\
Jan 15 10:30:00 ubuntu sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/id
Jan 15 10:30:05 ubuntu sudo: pam_unix(sudo:session): session opened for user root by alice(uid=0)";
    let events =
        parse_auth_log_sudo(input, &LogTimeHint::utc(None)).expect("should parse auth log");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].command, "/usr/bin/id");
    assert!(events[0].success);
}

#[test]
fn parse_sudo_auth_failure() {
    let input = "\
Jan 15 10:30:00 ubuntu sudo: pam_unix(sudo:auth): authentication failure; logname=alice uid=1000 euid=0 tty=/dev/pts/0 ruser=alice rhost=  user=alice
Jan 15 10:30:01 ubuntu sudo:   alice : 3 incorrect password attempts ; TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/su";
    let events = parse_auth_log_sudo(input, &LogTimeHint::utc(None)).expect("should parse");
    let failure = events
        .iter()
        .find(|e| e.command == "[authentication failure]")
        .expect("authentication failure event");
    assert_eq!(failure.user, "alice");
    assert!(!failure.success);
}

#[test]
fn parse_empty_input() {
    assert!(parse_auth_log_sudo("", &LogTimeHint::utc(None))
        .expect("should parse")
        .is_empty());
}

#[test]
fn skip_non_sudo_lines() {
    let input = "\
Jan 15 10:30:00 ubuntu sshd[1234]: Accepted publickey for alice from 192.168.1.100 port 22
Jan 15 10:30:01 ubuntu CRON[5678]: (root) CMD (test -x /usr/sbin/anacron || ...)
Jan 15 10:30:02 ubuntu sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/whoami";
    let events = parse_auth_log_sudo(input, &LogTimeHint::utc(None)).expect("should parse");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].user, "alice");
}

#[test]
fn rhel_secure_log_format() {
    let input = "\
Jan 15 10:30:00 centos sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/yum update
Jan 15 10:30:05 centos sudo: pam_unix(sudo:session): session opened for user root by alice(uid=0)";
    let events =
        parse_auth_log_sudo(input, &LogTimeHint::utc(None)).expect("should parse RHEL secure log");
    let cmds: Vec<&SudoEvent> = events
        .iter()
        .filter(|e| !e.command.contains("authentication failure"))
        .collect();
    assert!(!cmds.is_empty());
    assert_eq!(cmds[0].command, "/usr/bin/yum update");
}

#[test]
fn syslog_timestamp_uses_reference_year() {
    let input = "\
Jan 15 10:30:00 ubuntu sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/id";
    let events = parse_auth_log_sudo_with_reference(input, reference(2024, 1, 20), &UtcClock)
        .expect("should parse");
    assert_eq!(events.len(), 1);
    let ts = events[0].timestamp.expect("timestamp must parse");
    assert_eq!(ts.to_rfc3339(), "2024-01-15T10:30:00+00:00");
}

#[test]
fn syslog_timestamp_rolls_back_year_when_after_reference() {
    // A log acquired in January still holds December entries: the parsed
    // date must not land in the future relative to the reference.
    let input = "\
Dec 31 23:59:00 ubuntu sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/id";
    let events = parse_auth_log_sudo_with_reference(input, reference(2024, 1, 5), &UtcClock)
        .expect("should parse");
    assert_eq!(events.len(), 1);
    let ts = events[0].timestamp.expect("timestamp must parse");
    assert_eq!(ts.to_rfc3339(), "2023-12-31T23:59:00+00:00");
}

#[test]
fn iso8601_timestamp_is_parsed() {
    let input = "\
2024-01-15T10:30:00.123456+00:00 ubuntu sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/id";
    let events = parse_auth_log_sudo_with_reference(input, reference(2024, 6, 1), &UtcClock)
        .expect("should parse");
    assert_eq!(events.len(), 1);
    let ts = events[0].timestamp.expect("ISO timestamp must parse");
    assert_eq!(ts.timestamp(), 1_705_314_600);
    assert_eq!(ts.timestamp_subsec_micros(), 123_456);
}

#[test]
fn sudo_pid_tag_variant_is_accepted() {
    let input = "\
Jan 15 10:30:00 ubuntu sudo[4321]:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/id";
    let events = parse_auth_log_sudo_with_reference(input, reference(2024, 1, 20), &UtcClock)
        .expect("should parse");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].user, "alice");
    assert_eq!(events[0].command, "/usr/bin/id");
}

#[test]
fn lines_from_other_tags_mentioning_sudo_are_ignored() {
    let input = "\
Jan 15 10:30:00 ubuntu sudoedit:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/etc/hosts
Jan 15 10:30:01 ubuntu systemd[1]: Started sudo session for alice ; COMMAND=/usr/bin/id
Jan 15 10:30:02 ubuntu cron[999]: sudo-like message COMMAND=/usr/bin/false
Jan 15 10:30:03 ubuntu sudo:   bob : TTY=pts/1 ; PWD=/home/bob ; USER=root ; COMMAND=/usr/bin/whoami";
    let events = parse_auth_log_sudo_with_reference(input, reference(2024, 1, 20), &UtcClock)
        .expect("should parse");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].user, "bob");
    assert_eq!(events[0].command, "/usr/bin/whoami");
}

#[test]
fn session_open_close_lines_are_not_emitted() {
    let input = "\
Jan 15 10:30:05 ubuntu sudo: pam_unix(sudo:session): session opened for user root by alice(uid=0)
Jan 15 10:35:00 ubuntu sudo: pam_unix(sudo:session): session closed for user root";
    let events = parse_auth_log_sudo_with_reference(input, reference(2024, 1, 20), &UtcClock)
        .expect("should parse");
    assert!(events.is_empty());
}

/// +08:00 clock mirroring a host in Asia/Shanghai (no DST history).
struct PlusEightClock;

impl LogClock for PlusEightClock {
    fn local_to_utc(&self, local: NaiveDateTime) -> Option<DateTime<Utc>> {
        let offset = FixedOffset::east_opt(8 * 3600).expect("valid offset");
        offset
            .from_local_datetime(&local)
            .single()
            .map(|dt| dt.with_timezone(&Utc))
    }
    fn utc_to_local_naive(&self, timestamp: DateTime<Utc>) -> NaiveDateTime {
        timestamp
            .with_timezone(&FixedOffset::east_opt(8 * 3600).expect("valid offset"))
            .naive_local()
    }
}

#[test]
fn syslog_timestamp_converts_with_host_clock() {
    // Host in Asia/Shanghai (+08:00): the naive wall-clock line must land
    // 8 hours earlier once converted to UTC.
    let input = "\
Jan 15 10:30:00 ubuntu sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/id";
    let hint = LogTimeHint {
        reference: Some(reference(2024, 1, 20)),
        clock: &PlusEightClock,
    };
    let events = parse_auth_log_sudo(input, &hint).expect("should parse");
    assert_eq!(events.len(), 1);
    let ts = events[0].timestamp.expect("timestamp must parse");
    assert_eq!(ts.to_rfc3339(), "2024-01-15T02:30:00+00:00");
}

#[test]
fn year_rollover_is_decided_in_local_time_before_conversion() {
    // Reference 2024-01-05T12:00:00Z is 2024-01-05 20:00 local (+08:00). The
    // "Dec 31 23:59" line is after that in local time, so it rolls back to
    // 2023 *before* the UTC conversion.
    let input = "\
Dec 31 23:59:00 ubuntu sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/id";
    let hint = LogTimeHint {
        reference: Some(reference(2024, 1, 5)),
        clock: &PlusEightClock,
    };
    let events = parse_auth_log_sudo(input, &hint).expect("should parse");
    assert_eq!(events.len(), 1);
    let ts = events[0].timestamp.expect("timestamp must parse");
    assert_eq!(ts.to_rfc3339(), "2023-12-31T15:59:00+00:00");
}
