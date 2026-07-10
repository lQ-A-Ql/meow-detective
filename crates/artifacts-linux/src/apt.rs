//! APT package management log parser.
//!
//! Parses package-manager log formats:
//! - `/var/log/apt/history.log` — high-level APT transactions (install/upgrade/remove).
//! - `/var/log/dpkg.log` — low-level dpkg operations (install/configure/remove/purge).
//! - `/var/log/yum.log` and `/var/log/dnf*.log` — RHEL/CentOS/Fedora package events.
//!
//! APT history.log format:
//! ```text
//! Start-Date: 2024-01-15  10:30:00
//! Commandline: apt-get install curl
//! Install: libcurl4:amd64 (7.88.1-10+deb12u5), ...
//! End-Date: 2024-01-15  10:30:15
//! ```
//!
//! dpkg.log format:
//! ```text
//! 2024-01-15 10:30:00 install curl:amd64 7.88.1-10+deb12u5
//! 2024-01-15 10:30:05 configure curl:amd64 7.88.1-10+deb12u5 <none>
//! ```

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// An APT package management event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AptEvent {
    /// Action performed (install, upgrade, remove, purge, configure, etc.)
    pub action: String,
    /// Package name (may include architecture suffix like ":amd64")
    pub package: String,
    /// Package version
    pub version: String,
    /// Event timestamp
    pub timestamp: Option<DateTime<Utc>>,
}

/// Parse `/var/log/apt/history.log` content.
///
/// Extracts Start-Date/End-Date transaction entries and returns individual
/// package events for each Install/Upgrade/Remove/Purge line.
pub fn parse_apt_history(content: &str) -> Result<Vec<AptEvent>, crate::LinuxArtifactError> {
    let mut events: Vec<AptEvent> = Vec::new();
    let mut current_timestamp: Option<DateTime<Utc>> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Parse Start-Date
        if let Some(date_str) = trimmed.strip_prefix("Start-Date: ") {
            current_timestamp = parse_apt_date(date_str);
            continue;
        }

        // Parse action lines: Install:, Upgrade:, Remove:, Purge:, Reinstall:
        let (action, packages_str) = if let Some(s) = trimmed.strip_prefix("Install: ") {
            ("install", s)
        } else if let Some(s) = trimmed.strip_prefix("Upgrade: ") {
            ("upgrade", s)
        } else if let Some(s) = trimmed.strip_prefix("Remove: ") {
            ("remove", s)
        } else if let Some(s) = trimmed.strip_prefix("Purge: ") {
            ("purge", s)
        } else if let Some(s) = trimmed.strip_prefix("Reinstall: ") {
            ("reinstall", s)
        } else if let Some(s) = trimmed.strip_prefix("Downgrade: ") {
            ("downgrade", s)
        } else {
            continue;
        };

        for item in packages_str.split(',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let (package, version) = parse_package_entry(item);
            events.push(AptEvent {
                action: action.to_string(),
                package,
                version,
                timestamp: current_timestamp,
            });
        }
    }

    Ok(events)
}

/// Parse `/var/log/dpkg.log` content.
///
/// Each line has the format:
/// ```text
/// YYYY-MM-DD HH:MM:SS action package version [prev_version]
/// ```
pub fn parse_dpkg_log(content: &str) -> Result<Vec<AptEvent>, crate::LinuxArtifactError> {
    let mut events: Vec<AptEvent> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // dpkg.log lines have a leading timestamp: "2024-01-15 10:30:00 action ..."
        let parts: Vec<&str> = trimmed.splitn(5, ' ').collect();
        if parts.len() < 4 {
            continue;
        }

        // Parse timestamp: YYYY-MM-DD HH:MM:SS
        let date_str = format!("{} {}", parts[0], parts[1]);
        let timestamp = NaiveDateTime::parse_from_str(&date_str, "%Y-%m-%d %H:%M:%S")
            .ok()
            .map(|ndt| Utc.from_utc_datetime(&ndt));

        let action = parts[2].to_string();
        let package = parts[3].split(':').next().unwrap_or("unknown").to_string();

        let version = if parts.len() >= 5 {
            parts[4]
                .split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_string()
        } else {
            "unknown".to_string()
        };

        events.push(AptEvent {
            action,
            package,
            version,
            timestamp,
        });
    }

    Ok(events)
}

/// Parse RHEL/CentOS/Fedora yum/dnf package logs.
///
/// The returned DTO family is still named `AptEvent` for historical frontend
/// compatibility; semantically these are generic Linux package-manager events.
pub fn parse_rpm_package_log(content: &str) -> Result<Vec<AptEvent>, crate::LinuxArtifactError> {
    let mut events = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some((action, package_token)) = parse_rpm_action_line(trimmed) else {
            continue;
        };
        let (package, version) = parse_rpm_nevra(package_token);
        events.push(AptEvent {
            action,
            package,
            version,
            timestamp: parse_rpm_timestamp(trimmed),
        });
    }

    Ok(events)
}

fn parse_apt_date(s: &str) -> Option<DateTime<Utc>> {
    // Format: "2024-01-15  10:30:00" (note: double space between date and time)
    let normalized = s.replace("  ", " ");
    NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|ndt| Utc.from_utc_datetime(&ndt))
}

fn parse_rpm_action_line(line: &str) -> Option<(String, &str)> {
    for raw_action in [
        "Installed:",
        "Updated:",
        "Upgraded:",
        "Erased:",
        "Removed:",
        "Downgraded:",
        "Reinstalled:",
    ] {
        if let Some((_, rest)) = line.split_once(raw_action) {
            let package = rest.split_whitespace().next()?;
            let action = match raw_action
                .trim_end_matches(':')
                .to_ascii_lowercase()
                .as_str()
            {
                "installed" => "install".to_string(),
                "updated" | "upgraded" => "upgrade".to_string(),
                "erased" | "removed" => "remove".to_string(),
                "downgraded" => "downgrade".to_string(),
                "reinstalled" => "reinstall".to_string(),
                value => value.to_string(),
            };
            return Some((action, package));
        }
    }
    None
}

fn parse_rpm_timestamp(line: &str) -> Option<DateTime<Utc>> {
    let first = line.split_whitespace().next()?;
    DateTime::parse_from_str(first, "%Y-%m-%dT%H:%M:%S%z")
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

fn parse_rpm_nevra(token: &str) -> (String, String) {
    let cleaned = token.trim().trim_matches(',');
    let parts = cleaned.split('-').collect::<Vec<_>>();
    if parts.len() >= 3 {
        let package = parts[..parts.len() - 2].join("-");
        let version = parts[parts.len() - 2..].join("-");
        if !package.is_empty() {
            return (package, version);
        }
    }
    (cleaned.to_string(), "unknown".to_string())
}

fn parse_package_entry(entry: &str) -> (String, String) {
    // Format: "libcurl4:amd64 (7.88.1-10+deb12u5), automatic"
    // or just: "libcurl4:amd64 (7.88.1-10+deb12u5)"
    let entry = entry.trim();
    if let Some(paren_open) = entry.find('(') {
        let package = entry[..paren_open].trim().to_string();
        let rest = &entry[paren_open + 1..];
        if let Some(paren_close) = rest.find(')') {
            let version = rest[..paren_close].trim().to_string();
            return (package, version);
        }
    }
    (entry.to_string(), "unknown".to_string())
}

#[cfg(test)]
mod tests {
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
}
