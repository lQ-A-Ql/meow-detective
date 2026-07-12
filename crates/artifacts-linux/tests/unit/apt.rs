use super::*;

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
    assert_eq!(events[0].version, "7.88.1-10+deb12u5");
    assert!(events[0].timestamp.is_some());
    assert_eq!(events[1].action, "install");
    assert_eq!(events[1].package, "vim:amd64");
    assert_eq!(events[1].version, "2:9.0.1378-2");
    assert!(events[1].timestamp.is_some());
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
2024-01-15 10:30:00 install curl:amd64 7.88.1-10+deb12u5
2024-01-15 10:30:01 configure curl:amd64 7.88.1-10+deb12u5 <none>
2024-01-15 10:30:05 status half-installed curl:amd64 7.88.1-9
2024-06-01 14:00:00 upgrade libssl3:amd64 3.0.11-1~deb12u2 <none>
2024-06-02 09:15:03 remove old-package:amd64 1.0.0-1 <none>";

    let events = parse_dpkg_log(input).expect("should parse dpkg log");
    assert_eq!(events.len(), 5);
    assert_eq!(events[0].action, "install");
    assert_eq!(events[0].package, "curl");
    assert_eq!(events[0].version, "7.88.1-10+deb12u5");
    assert_eq!(events[1].action, "configure");
    assert_eq!(events[4].action, "remove");
}

#[test]
fn parse_rpm_package_logs() {
    let input = "\
Jan 15 10:30:00 Installed: curl-7.61.1-33.el8.x86_64
2024-01-15T10:31:00+0000 INFO --- Updated: python3-libdnf-0.63.0-20.el8.x86_64
2024-01-15T10:32:00+0000 INFO --- Erased: oldpkg-1.0-1.el8.x86_64
";

    let events = parse_rpm_package_log(input).expect("should parse rpm package logs");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].action, "install");
    assert_eq!(events[0].package, "curl");
    assert_eq!(events[0].version, "7.61.1-33.el8.x86_64");
    assert!(events[0].timestamp.is_none());
    assert_eq!(events[1].action, "upgrade");
    assert_eq!(events[1].package, "python3-libdnf");
    assert!(events[1].timestamp.is_some());
    assert_eq!(events[2].action, "remove");
}

#[test]
fn parse_empty_input() {
    let events = parse_apt_history("").expect("should parse");
    assert!(events.is_empty());
    let events = parse_dpkg_log("").expect("should parse");
    assert!(events.is_empty());
}
