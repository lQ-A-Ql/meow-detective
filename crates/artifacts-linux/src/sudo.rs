//! sudo log parser for `/var/log/auth.log` (or `/var/log/secure` on RHEL).
//!
//! Parses sudo-related events from auth logs. Supports typical distributions:
//! - Debian/Ubuntu: `/var/log/auth.log` (rsyslog format)
//! - RHEL/CentOS: `/var/log/secure`
//!
//! Typical sudo log lines:
//! ```text
//! Jan 15 10:30:00 hostname sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/apt update
//! Jan 15 10:30:05 hostname sudo: pam_unix(sudo:session): session opened for user root by alice(uid=0)
//! Jan 15 10:32:00 hostname sudo: pam_unix(sudo:session): session closed for user root
//! ```
//!
//! This parser extracts COMMAND= execution lines and authentication failures.
//! `pam_unix(sudo:session)` session open/close lines are recognized but
//! deliberately not emitted as events: they duplicate the COMMAND= record and
//! add no investigative value beyond it.

use chrono::{DateTime, Datelike, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// A sudo command execution event extracted from auth logs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SudoEvent {
    /// The user who invoked sudo
    pub user: String,
    /// The command that was executed via sudo
    pub command: String,
    /// Working directory at time of sudo
    pub working_directory: Option<String>,
    /// Target user (usually "root")
    pub target_user: Option<String>,
    /// Terminal from which sudo was invoked
    pub terminal: Option<String>,
    /// Timestamp of the sudo event
    pub timestamp: Option<DateTime<Utc>>,
    /// Whether the sudo session was successful (session opened)
    pub success: bool,
}

/// Parse `/var/log/auth.log` (or `/var/log/secure`) for sudo events.
///
/// `reference` anchors year-less syslog timestamps (typically the log file's
/// mtime from the evidence file entry). When `None`, the current time is
/// used. A parsed timestamp that would land after the reference is moved
/// back one year, because syslog timestamps carry no year.
///
/// Only lines whose syslog tag is exactly `sudo` (`sudo:` or `sudo[pid]:`)
/// are considered; this avoids false positives from other daemons whose
/// message text merely mentions "sudo".
pub fn parse_auth_log_sudo(
    content: &str,
    reference: Option<DateTime<Utc>>,
) -> Result<Vec<SudoEvent>, crate::LinuxArtifactError> {
    parse_auth_log_sudo_with_reference(content, reference.unwrap_or_else(Utc::now))
}

/// Parse an auth log for sudo events with an explicit reference time.
///
/// Equivalent to [`parse_auth_log_sudo`] with `Some(reference)`; convenient
/// for tests and forensic flows that always have an anchor time.
pub fn parse_auth_log_sudo_with_reference(
    content: &str,
    reference: DateTime<Utc>,
) -> Result<Vec<SudoEvent>, crate::LinuxArtifactError> {
    let mut events: Vec<SudoEvent> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some(message) = split_sudo_message(trimmed) else {
            continue;
        };
        let timestamp = parse_log_timestamp(trimmed, reference);

        // Session open/close lines are recognized but intentionally dropped.
        if message.contains("session opened for user")
            || message.contains("session closed for user")
        {
            continue;
        }

        // ---- COMMAND= line ----
        if let Some(command_part) = message.split("COMMAND=").nth(1) {
            let command = command_part.trim().to_string();

            // Extract fields: TTY=..., PWD=..., USER=...
            let tty = extract_sudo_field(message, "TTY=");
            let pwd = extract_sudo_field(message, "PWD=");
            let target_user_field = extract_sudo_field(message, "USER=");
            let user = extract_invoking_user(message);

            if !command.is_empty() {
                events.push(SudoEvent {
                    user,
                    command,
                    working_directory: if pwd.is_empty() { None } else { Some(pwd) },
                    target_user: if target_user_field.is_empty() {
                        None
                    } else {
                        Some(target_user_field)
                    },
                    terminal: if tty.is_empty() { None } else { Some(tty) },
                    timestamp,
                    // A plain COMMAND= line is emitted after sudo
                    // authorization and records an executed command. Some
                    // auth-failure formats also include COMMAND=; keep those
                    // explicitly failed.
                    success: !is_sudo_auth_failure(message),
                });
            }
            continue;
        }

        // ---- unsuccessful sudo attempt ----
        if is_sudo_auth_failure(message) {
            let user = extract_sudo_user(message);
            if !user.is_empty() {
                events.push(SudoEvent {
                    user,
                    command: "[authentication failure]".to_string(),
                    working_directory: None,
                    target_user: Some("root".to_string()),
                    terminal: None,
                    timestamp,
                    success: false,
                });
            }
        }
    }

    Ok(events)
}

fn is_sudo_auth_failure(line: &str) -> bool {
    line.contains("authentication failure")
        || line.contains("incorrect password")
        || line.contains("3 incorrect password attempts")
}

/// Split a syslog line into (sudo tag match, message after the tag).
///
/// The tag must appear within the first five whitespace-separated fields
/// (timestamp tokens + hostname) and be exactly `sudo:` or `sudo[<pid>]:`.
/// Returns the message portion when the line is a genuine sudo log line.
fn split_sudo_message(line: &str) -> Option<&str> {
    let mut search_from = 0usize;
    for (index, field) in line.split_whitespace().enumerate() {
        // Tag position: ISO ts (1 token) or syslog ts (3 tokens) + hostname.
        if index > 4 {
            return None;
        }
        let pos = line[search_from..].find(field)? + search_from;
        search_from = pos + field.len();
        if field == "sudo:" || is_sudo_pid_tag(field) {
            return Some(line[search_from..].trim_start());
        }
    }
    None
}

fn is_sudo_pid_tag(field: &str) -> bool {
    if !(field.starts_with("sudo[") && field.ends_with("]:")) {
        return false;
    }
    let pid = &field["sudo[".len()..field.len() - 2];
    !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit())
}

/// Parse the timestamp at the start of a log line.
///
/// - ISO 8601 / RFC 3339 (systemd journal export): the first
///   whitespace-separated token is parsed directly.
/// - Standard syslog `Mon DD HH:MM:SS` (15 bytes, no year): the year is taken
///   from `reference`; if the result is later than `reference`, it is moved
///   back one year (a log written in January can hold December entries).
fn parse_log_timestamp(line: &str, reference: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if let Some(first) = line.split_whitespace().next() {
        if let Ok(dt) = DateTime::parse_from_rfc3339(first) {
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

/// Extract the invoking user from the sudo message body.
///
/// COMMAND lines look like `alice : TTY=pts/0 ; ... COMMAND=/usr/bin/id`.
fn extract_invoking_user(message: &str) -> String {
    if let Some((head, _)) = message.split_once(" :") {
        let user = head.trim();
        if !user.is_empty() && !user.contains("pam_unix") {
            return user.to_string();
        }
    }
    message
        .split_whitespace()
        .next()
        .unwrap_or("unknown")
        .to_string()
}

fn extract_sudo_field(line: &str, key: &str) -> String {
    if let Some(rest) = line.split(key).nth(1) {
        // Value is followed by space or semicolon
        rest.split([' ', ';'])
            .next()
            .unwrap_or("")
            .trim()
            .to_string()
    } else {
        String::new()
    }
}

fn extract_sudo_user(line: &str) -> String {
    // Try to extract the user from sudo-related auth failure lines
    // Format: "...sudo: pam_unix(sudo:auth): authentication failure; logname=alice uid=1000..."
    if let Some(rest) = line.split("logname=").nth(1) {
        return rest
            .split_whitespace()
            .next()
            .unwrap_or("unknown")
            .to_string();
    }
    // Fallback: try user= in some formats
    if let Some(rest) = line.split("user=").nth(1) {
        return rest
            .split_whitespace()
            .next()
            .unwrap_or("unknown")
            .to_string();
    }
    String::new()
}

#[cfg(test)]
#[path = "../tests/unit/sudo.rs"]
mod tests;
