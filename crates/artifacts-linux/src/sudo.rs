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
pub fn parse_auth_log_sudo(content: &str) -> Result<Vec<SudoEvent>, String> {
    let mut events: Vec<SudoEvent> = Vec::new();
    // Track session opens to mark command lines successful
    let mut session_open_users: Vec<(String, String)> = Vec::new(); // (target_user, invoking_user)

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

        // ---- session opened ----
        // Format: "...sudo: pam_unix(sudo:session): session opened for user root by alice(uid=0)"
        if let Some(rest) = trimmed.split("session opened for user ").nth(1) {
            let target_user = rest.split_whitespace().next().unwrap_or("unknown");
            let invoking_user = rest
                .split(" by ")
                .nth(1)
                .and_then(|s| s.split('(').next())
                .unwrap_or("unknown");
            session_open_users.push((target_user.to_string(), invoking_user.to_string()));
            continue;
        }

        // ---- session closed ----
        if trimmed.contains("session closed for user") {
            let target_user = trimmed
                .split("session closed for user ")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .unwrap_or("unknown");
            // Remove matching session open
            session_open_users.retain(|(tu, _)| tu != target_user);
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

            // Check if this user/target has an active session
            let success = !command.is_empty()
                && session_open_users
                    .iter()
                    .any(|(tu, iu)| tu == &target_user_field && iu == &user);

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
                    success,
                });
            }
            continue;
        }

        // ---- unsuccessful sudo attempt ----
        if trimmed.contains("authentication failure")
            || trimmed.contains("incorrect password")
            || trimmed.contains("3 incorrect password attempts")
        {
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
mod tests {
    use super::*;

    #[test]
    fn parse_sudo_command_lines() {
        let input = "\
Jan 15 10:30:00 ubuntu sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/apt update
Jan 15 10:30:05 ubuntu sudo: pam_unix(sudo:session): session opened for user root by alice(uid=0)
Jan 15 10:32:00 ubuntu sudo:     bob : TTY=pts/1 ; PWD=/home/bob ; USER=root ; COMMAND=/usr/bin/systemctl restart nginx
Jan 15 10:32:05 ubuntu sudo: pam_unix(sudo:session): session opened for user root by bob(uid=0)
Jan 15 10:35:00 ubuntu sudo: pam_unix(sudo:session): session closed for user root";

        let events = parse_auth_log_sudo(input).expect("should parse auth log");
        assert!(!events.is_empty(), "should find sudo events");

        let cmds: Vec<&SudoEvent> = events
            .iter()
            .filter(|e| !e.command.contains("authentication failure"))
            .collect();
        assert_eq!(cmds.len(), 2);

        let alice_event = &cmds[0];
        assert_eq!(alice_event.user, "alice");
        assert_eq!(alice_event.command, "/usr/bin/apt update");
        assert_eq!(
            alice_event.working_directory.as_deref(),
            Some("/home/alice")
        );
        assert_eq!(alice_event.target_user.as_deref(), Some("root"));
        assert_eq!(alice_event.terminal.as_deref(), Some("pts/0"));

        let bob_event = &cmds[1];
        assert_eq!(bob_event.user, "bob");
        assert_eq!(bob_event.command, "/usr/bin/systemctl restart nginx");
    }

    #[test]
    fn parse_sudo_auth_failure() {
        let input = "\
Jan 15 10:30:00 ubuntu sudo: pam_unix(sudo:auth): authentication failure; logname=alice uid=1000 euid=0 tty=/dev/pts/0 ruser=alice rhost=  user=alice
Jan 15 10:30:01 ubuntu sudo:   alice : 3 incorrect password attempts ; TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/su";

        let events = parse_auth_log_sudo(input).expect("should parse");
        assert!(!events.is_empty());
        let failure = events
            .iter()
            .find(|e| e.command == "[authentication failure]");
        assert!(failure.is_some());
        assert_eq!(failure.unwrap().user, "alice");
        assert!(!failure.unwrap().success);
    }

    #[test]
    fn parse_empty_input() {
        let events = parse_auth_log_sudo("").expect("should parse");
        assert!(events.is_empty());
    }

    #[test]
    fn skip_non_sudo_lines() {
        let input = "\
Jan 15 10:30:00 ubuntu sshd[1234]: Accepted publickey for alice from 192.168.1.100 port 22
Jan 15 10:30:01 ubuntu CRON[5678]: (root) CMD (test -x /usr/sbin/anacron || ...)
Jan 15 10:30:02 ubuntu sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/whoami";

        let events = parse_auth_log_sudo(input).expect("should parse");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].user, "alice");
    }

    #[test]
    fn rhel_secure_log_format() {
        // RHEL-based systems use /var/log/secure
        let input = "\
Jan 15 10:30:00 centos sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/yum update
Jan 15 10:30:05 centos sudo: pam_unix(sudo:session): session opened for user root by alice(uid=0)";

        let events = parse_auth_log_sudo(input).expect("should parse RHEL secure log");
        let cmds: Vec<&SudoEvent> = events
            .iter()
            .filter(|e| !e.command.contains("authentication failure"))
            .collect();
        assert!(!cmds.is_empty());
        assert_eq!(cmds[0].command, "/usr/bin/yum update");
    }
}
