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
//! This parser focuses on the COMMAND= lines which record the actual sudo executions.

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
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
/// Looks for lines containing `sudo:` and extracts COMMAND= entries
/// as well as session open/close lines.
pub fn parse_auth_log_sudo(content: &str) -> Result<Vec<SudoEvent>, crate::LinuxArtifactError> {
    let mut events: Vec<SudoEvent> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Only process lines containing sudo
        if !trimmed.contains("sudo") && !trimmed.contains("sudo:") {
            continue;
        }

        let timestamp = parse_syslog_timestamp(trimmed);

        if trimmed.contains("session opened for user")
            || trimmed.contains("session closed for user")
        {
            continue;
        }

        // ---- COMMAND= line ----
        if let Some(command_part) = trimmed.split("COMMAND=").nth(1) {
            let command = command_part.trim().to_string();

            // Extract fields: TTY=..., PWD=..., USER=...
            let tty = extract_sudo_field(trimmed, "TTY=");
            let pwd = extract_sudo_field(trimmed, "PWD=");
            let target_user_field = extract_sudo_field(trimmed, "USER=");

            // Extract the invoking user (before the colon in "username :")
            let user = if let Some(user_part) = trimmed.split("sudo:").nth(1) {
                let user_part = user_part.trim();
                if let Some(colon_pos) = user_part.find(" :") {
                    user_part[..colon_pos].trim().to_string()
                } else if user_part.contains("pam_unix") {
                    // Not a COMMAND line handled here
                    String::new()
                } else {
                    user_part
                        .split_whitespace()
                        .next()
                        .unwrap_or("unknown")
                        .to_string()
                }
            } else {
                "unknown".to_string()
            };

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
                    success: !is_sudo_auth_failure(trimmed),
                });
            }
            continue;
        }

        // ---- unsuccessful sudo attempt ----
        if is_sudo_auth_failure(trimmed) {
            let user = extract_sudo_user(trimmed);
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

/// Parse a syslog-style timestamp at the start of a line.
/// Handles formats like:
/// - "Jan 15 10:30:00" (standard syslog, no year — we assume current-ish year)
/// - "2024-01-15T10:30:00.000000+00:00" (ISO 8601 from systemd journal)
/// - "Jan 15 10:30:00 hostname"
fn parse_syslog_timestamp(line: &str) -> Option<DateTime<Utc>> {
    // Try ISO 8601 first (systemd journal export format)
    if line.len() >= 19 && line.as_bytes().get(4) == Some(&b'-') {
        if let Ok(dt) = DateTime::parse_from_rfc3339(&line[..line.len().min(35)]) {
            return Some(dt.with_timezone(&Utc));
        }
    }

    // Try standard syslog: "Mon DD HH:MM:SS"
    // Month abbreviations
    if line.len() >= 15 {
        let ts_str = &line[..15];
        if let Ok(ndt) =
            NaiveDateTime::parse_from_str(&format!("2024 {}:00", ts_str), "%Y %b %d %H:%M:%S")
        {
            return Some(Utc.from_utc_datetime(&ndt));
        }
    }

    None
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
