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
//! Requested-By: alice (1000)
//! Install: libcurl4:amd64 (7.88.1-10+deb12u5), ...
//! End-Date: 2024-01-15  10:30:15
//! ```
//!
//! dpkg.log format (note the old-version / new-version column pair):
//! ```text
//! 2024-01-15 10:30:00 install curl:amd64 <none> 7.88.1-10+deb12u5
//! 2024-01-15 10:30:01 configure curl:amd64 7.88.1-10+deb12u5 <none>
//! ```
//!
//! Conventions and known limitations:
//! - Package names keep their full `pkg:arch` form on every code path, so
//!   cross-log correlation can match on the exact original token.
//! - APT history and dpkg log timestamps are written in the system's local
//!   time zone, but the zone is not recorded in the log. They are parsed as
//!   if they were UTC; treat the resulting timestamps as approximate.

use chrono::{DateTime, Datelike, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// An APT package management event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AptEvent {
    /// Action performed (install, upgrade, remove, purge, configure, etc.)
    pub action: String,
    /// Package name in its full `pkg:arch` form (e.g. "curl:amd64")
    pub package: String,
    /// Package version relevant to the action: the new version for
    /// install/upgrade, the removed/configured version otherwise. `None`
    /// when the log records `<none>` or no version at all.
    pub version: Option<String>,
    /// Event timestamp (see the module docs for the local-time caveat)
    pub timestamp: Option<DateTime<Utc>>,
    /// User who requested the APT transaction (`Requested-By:` line)
    pub requested_by: Option<String>,
    /// Full command line of the APT transaction (`Commandline:` line)
    pub command_line: Option<String>,
}

/// dpkg.log actions that produce events. `status` progress lines carry no
/// forensic value (their first token is a dpkg state such as
/// `half-installed`, not a package action) and are skipped.
const DPKG_ACTIONS: &[&str] = &[
    "startup",
    "install",
    "upgrade",
    "configure",
    "trigproc",
    "remove",
    "purge",
];

/// Parse `/var/log/apt/history.log` content.
///
/// Extracts Start-Date/End-Date transaction entries and returns individual
/// package events for each Install/Upgrade/Remove/Purge line. The
/// transaction's `Commandline:` and `Requested-By:` lines are attached to
/// every event of that transaction.
pub fn parse_apt_history(content: &str) -> Result<Vec<AptEvent>, crate::LinuxArtifactError> {
    let mut events: Vec<AptEvent> = Vec::new();
    let mut current_timestamp: Option<DateTime<Utc>> = None;
    let mut current_command_line: Option<String> = None;
    let mut current_requested_by: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Parse Start-Date; each transaction resets the per-block metadata.
        if let Some(date_str) = trimmed.strip_prefix("Start-Date: ") {
            current_timestamp = parse_apt_date(date_str);
            current_command_line = None;
            current_requested_by = None;
            continue;
        }
        if let Some(command_line) = trimmed.strip_prefix("Commandline: ") {
            current_command_line = Some(command_line.trim().to_string());
            continue;
        }
        if let Some(requested_by) = trimmed.strip_prefix("Requested-By: ") {
            current_requested_by = Some(requested_by.trim().to_string());
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
                requested_by: current_requested_by.clone(),
                command_line: current_command_line.clone(),
            });
        }
    }

    Ok(events)
}

/// Parse `/var/log/dpkg.log` content.
///
/// Each action line has the format:
/// ```text
/// YYYY-MM-DD HH:MM:SS action package:arch <old-version> <new-version>
/// ```
/// Only action lines listed in [`DPKG_ACTIONS`] produce events; `status`
/// progress lines are skipped.
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

        let action = parts[2];
        if !DPKG_ACTIONS.contains(&action) {
            continue;
        }

        // Parse timestamp: YYYY-MM-DD HH:MM:SS (local time, read as UTC —
        // see the module-level caveat).
        let date_str = format!("{} {}", parts[0], parts[1]);
        let timestamp = NaiveDateTime::parse_from_str(&date_str, "%Y-%m-%d %H:%M:%S")
            .ok()
            .map(|ndt| Utc.from_utc_datetime(&ndt));

        let package = parts[3].to_string();
        let version = select_dpkg_version(action, parts.get(4).copied().unwrap_or(""));

        events.push(AptEvent {
            action: action.to_string(),
            package,
            version,
            timestamp,
            requested_by: None,
            command_line: None,
        });
    }

    Ok(events)
}

/// Pick the forensically relevant version from a dpkg version column pair.
///
/// `install`/`upgrade` lines record `<old-version> <new-version>` and the
/// new version is the one being introduced; other actions (configure,
/// remove, purge, ...) refer to the first (current) version. A `<none>`
/// placeholder or a missing column yields `None`.
fn select_dpkg_version(action: &str, version_columns: &str) -> Option<String> {
    let mut columns = version_columns.split_whitespace();
    let picked = if action == "install" || action == "upgrade" {
        columns.next_back()
    } else {
        columns.next()
    }?;
    if picked == "<none>" {
        None
    } else {
        Some(picked.to_string())
    }
}

/// Parse RHEL/CentOS/Fedora yum/dnf package logs.
///
/// `reference` anchors the year-less syslog timestamps of yum.log (typically
/// the log file's mtime from the evidence file entry). When `None`, the
/// current time is used. A parsed timestamp that would land after the
/// reference is moved back one year, because syslog timestamps carry no
/// year. dnf.log RFC3339 timestamps are unaffected by the reference.
///
/// The returned DTO family is still named `AptEvent` for historical frontend
/// compatibility; semantically these are generic Linux package-manager events.
pub fn parse_rpm_package_log(
    content: &str,
    reference: Option<DateTime<Utc>>,
) -> Result<Vec<AptEvent>, crate::LinuxArtifactError> {
    parse_rpm_package_log_with_reference(content, reference.unwrap_or_else(Utc::now))
}

/// Parse yum/dnf package logs with an explicit reference time.
///
/// Equivalent to [`parse_rpm_package_log`] with `Some(reference)`; convenient
/// for tests and forensic flows that always have an anchor time.
pub fn parse_rpm_package_log_with_reference(
    content: &str,
    reference: DateTime<Utc>,
) -> Result<Vec<AptEvent>, crate::LinuxArtifactError> {
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
            timestamp: parse_rpm_timestamp(trimmed, reference),
            requested_by: None,
            command_line: None,
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

/// Parse the timestamp of a yum/dnf log line.
///
/// - dnf.log leads with an RFC3339 token, parsed directly.
/// - yum.log uses classic syslog `Mon DD HH:MM:SS` (15 bytes, no year): the
///   year is taken from `reference`; if the result is later than `reference`
///   it is moved back one year (a log rotated in January can still hold
///   December entries).
fn parse_rpm_timestamp(line: &str, reference: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if let Some(first) = line.split_whitespace().next() {
        if let Ok(dt) = DateTime::parse_from_str(first, "%Y-%m-%dT%H:%M:%S%z") {
            return Some(dt.with_timezone(&Utc));
        }
    }

    let prefix = line.get(..15)?;
    let year = reference.year();
    let stamped = format!("{year} {prefix}");
    let naive = NaiveDateTime::parse_from_str(&stamped, "%Y %b %e %H:%M:%S").ok()?;
    let dt = Utc.from_utc_datetime(&naive);
    if dt > reference {
        if let Some(shifted) = naive.with_year(year - 1) {
            return Some(Utc.from_utc_datetime(&shifted));
        }
    }
    Some(dt)
}

fn parse_rpm_nevra(token: &str) -> (String, Option<String>) {
    let cleaned = token.trim().trim_matches(',');
    let parts = cleaned.split('-').collect::<Vec<_>>();
    if parts.len() >= 3 {
        let package = parts[..parts.len() - 2].join("-");
        let version = parts[parts.len() - 2..].join("-");
        if !package.is_empty() {
            return (package, Some(version));
        }
    }
    (cleaned.to_string(), None)
}

fn parse_package_entry(entry: &str) -> (String, Option<String>) {
    // Format: "libcurl4:amd64 (7.88.1-10+deb12u5), automatic"
    // or just: "libcurl4:amd64 (7.88.1-10+deb12u5)"
    let entry = entry.trim();
    if let Some(paren_open) = entry.find('(') {
        let package = entry[..paren_open].trim().to_string();
        let rest = &entry[paren_open + 1..];
        if let Some(paren_close) = rest.find(')') {
            let version = rest[..paren_close].trim().to_string();
            return (package, Some(version));
        }
    }
    (entry.to_string(), None)
}

#[cfg(test)]
#[path = "../tests/unit/apt.rs"]
mod tests;
