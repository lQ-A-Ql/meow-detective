use super::*;
use chrono::TimeZone;

fn reference(y: i32, m: u32, d: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
}

#[test]
fn parse_apt_history_install() {
    let input = "\
Start-Date: 2024-01-15  10:30:00
Commandline: apt-get install curl vim
Install: curl:amd64 (7.88.1-10+deb12u5), vim:amd64 (2:9.0.1378-2)
End-Date: 2024-01-15  10:30:15";

    let events = parse_apt_history(input).expect("should parse APT history");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].action, "install");
    assert_eq!(events[0].package, "curl:amd64");
    assert_eq!(events[0].version.as_deref(), Some("7.88.1-10+deb12u5"));
    assert!(events[0].timestamp.is_some());
    assert_eq!(
        events[0].command_line.as_deref(),
        Some("apt-get install curl vim")
    );
    assert_eq!(events[1].action, "install");
    assert_eq!(events[1].package, "vim:amd64");
    assert_eq!(events[1].version.as_deref(), Some("2:9.0.1378-2"));
    assert!(events[1].timestamp.is_some());
}

#[test]
fn parse_apt_history_requested_by_is_captured() {
    let input = "\
Start-Date: 2024-01-15  10:30:00
Commandline: apt-get install curl
Requested-By: alice (1000)
Install: curl:amd64 (7.88.1-10+deb12u5)
End-Date: 2024-01-15  10:30:15

Start-Date: 2024-01-16  09:00:00
Install: vim:amd64 (2:9.0.1378-2)
End-Date: 2024-01-16  09:00:05";

    let events = parse_apt_history(input).expect("should parse");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].requested_by.as_deref(), Some("alice (1000)"));
    assert_eq!(
        events[0].command_line.as_deref(),
        Some("apt-get install curl")
    );
    // Second transaction has no Requested-By/Commandline lines.
    assert_eq!(events[1].requested_by, None);
    assert_eq!(events[1].command_line, None);
}

#[test]
fn parse_apt_history_upgrade_and_remove() {
    let input = "\
Start-Date: 2024-06-01  14:00:00
Upgrade: libssl3:amd64 (3.0.11-1~deb12u2), openssl:amd64 (3.0.11-1~deb12u2)
End-Date: 2024-06-01  14:00:05

Start-Date: 2024-06-02  09:15:00
Remove: old-package:amd64 (1.0.0-1)
End-Date: 2024-06-02  09:15:03";

    let events = parse_apt_history(input).expect("should parse");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].action, "upgrade");
    assert_eq!(events[0].package, "libssl3:amd64");
    assert_eq!(events[1].action, "upgrade");
    assert_eq!(events[1].package, "openssl:amd64");
    assert_eq!(events[2].action, "remove");
    assert_eq!(events[2].package, "old-package:amd64");
}

#[test]
fn parse_dpkg_log_entries() {
    let input = "\
2024-01-15 10:30:00 install curl:amd64 <none> 7.88.1-10+deb12u5
2024-01-15 10:30:01 configure curl:amd64 7.88.1-10+deb12u5 <none>
2024-06-01 14:00:00 upgrade libssl3:amd64 3.0.10-1 3.0.11-1~deb12u2
2024-06-02 09:15:03 remove old-package:amd64 1.0.0-1 <none>";

    let events = parse_dpkg_log(input).expect("should parse dpkg log");
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].action, "install");
    // Package names keep their :arch suffix for cross-log correlation.
    assert_eq!(events[0].package, "curl:amd64");
    // install reports the *new* version (second version column).
    assert_eq!(events[0].version.as_deref(), Some("7.88.1-10+deb12u5"));
    assert_eq!(events[1].action, "configure");
    assert_eq!(events[1].version.as_deref(), Some("7.88.1-10+deb12u5"));
    // upgrade reports the new version, not the old one.
    assert_eq!(events[2].action, "upgrade");
    assert_eq!(events[2].version.as_deref(), Some("3.0.11-1~deb12u2"));
    assert_eq!(events[3].action, "remove");
    assert_eq!(events[3].version.as_deref(), Some("1.0.0-1"));
}

#[test]
fn dpkg_status_lines_are_skipped() {
    let input = "\
2024-01-15 10:30:00 startup archives unpack
2024-01-15 10:30:05 status half-installed curl:amd64 7.88.1-9
2024-01-15 10:30:06 status unpacked curl:amd64 7.88.1-10+deb12u5
2024-01-15 10:30:07 install curl:amd64 <none> 7.88.1-10+deb12u5
2024-01-15 10:30:08 trigproc man-db:amd64 2.9.1-1 <none>";

    let events = parse_dpkg_log(input).expect("should parse dpkg log");
    let actions: Vec<&str> = events.iter().map(|e| e.action.as_str()).collect();
    assert_eq!(actions, ["startup", "install", "trigproc"]);
    // No garbage event with action="status" / package="half-installed".
    assert!(events.iter().all(|e| e.package != "half-installed"));
}

#[test]
fn dpkg_install_without_new_version_yields_none() {
    let input = "2024-01-15 10:30:00 install curl:amd64 7.88.1-9 <none>";
    let events = parse_dpkg_log(input).expect("should parse dpkg log");
    assert_eq!(events.len(), 1);
    // A `<none>` placeholder must surface as None, not the string "<none>".
    assert_eq!(events[0].version, None);
}

#[test]
fn parse_rpm_package_logs() {
    let input = "\
Jan 15 10:30:00 Installed: curl-7.61.1-33.el8.x86_64
2024-01-15T10:31:00+0000 INFO --- Updated: python3-libdnf-0.63.0-20.el8.x86_64
2024-01-15T10:32:00+0000 INFO --- Erased: oldpkg-1.0-1.el8.x86_64
";

    let reference = reference(2024, 1, 20);
    let events = parse_rpm_package_log(input, Some(reference)).expect("should parse rpm logs");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].action, "install");
    assert_eq!(events[0].package, "curl");
    assert_eq!(events[0].version.as_deref(), Some("7.61.1-33.el8.x86_64"));
    // yum.log syslog timestamp anchored to the reference year.
    assert_eq!(
        events[0].timestamp.expect("yum timestamp").to_rfc3339(),
        "2024-01-15T10:30:00+00:00"
    );
    assert_eq!(events[1].action, "upgrade");
    assert_eq!(events[1].package, "python3-libdnf");
    // dnf.log RFC3339 timestamps ignore the reference.
    assert_eq!(
        events[1].timestamp.expect("dnf timestamp").to_rfc3339(),
        "2024-01-15T10:31:00+00:00"
    );
    assert_eq!(events[2].action, "remove");
}

#[test]
fn yum_syslog_timestamp_rolls_back_year_after_reference() {
    // A yum.log last modified in January still holds December entries from
    // the tail of the previous log year.
    let input = "Dec 20 03:22:01 Installed: kernel-3.10.0-1160.el7.x86_64\n";
    let events =
        parse_rpm_package_log(input, Some(reference(2024, 1, 5))).expect("should parse yum log");
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].timestamp.expect("timestamp").to_rfc3339(),
        "2023-12-20T03:22:01+00:00"
    );
}

#[test]
fn yum_syslog_timestamp_keeps_reference_year_when_not_in_future() {
    let input = "May 11 15:39:06 Installed: wget-1.14-18.el7.x86_64\n";
    let events =
        parse_rpm_package_log(input, Some(reference(2024, 6, 1))).expect("should parse yum log");
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].timestamp.expect("timestamp").to_rfc3339(),
        "2024-05-11T15:39:06+00:00"
    );
}

#[test]
fn parse_empty_input() {
    let events = parse_apt_history("").expect("should parse");
    assert!(events.is_empty());
    let events = parse_dpkg_log("").expect("should parse");
    assert!(events.is_empty());
}
