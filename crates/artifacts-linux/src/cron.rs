//! Crontab parser.
//!
//! Parses crontab files in the standard vixie-cron format, including:
//! - `/var/spool/cron/crontabs/<user>` (user crontabs)
//! - `/etc/crontab` (system crontab, which includes a user field)
//! - `/etc/cron.d/*` (drop-in cron files)
//!
//! Format:
//! ```text
//! # comment
//! SHELL=/bin/bash
//! PATH=/usr/local/sbin:/usr/local/bin:/sbin:/bin:/usr/sbin:/usr/bin
//!
//! # m h  dom mon dow   command
//! 30 2 * * * /usr/bin/some-script.sh
//! @daily /usr/bin/logrotate
//! 0 */6 * * * root /usr/bin/system-update
//! ```

use serde::{Deserialize, Serialize};

/// A cron job entry from a crontab file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CronJob {
    /// The cron schedule expression (e.g. "30 2 * * *" or "@daily")
    pub schedule: String,
    /// The user the job runs as (for system crontabs with user field)
    pub user: Option<String>,
    /// The command to execute
    pub command: String,
    /// Source file path (for identification)
    pub source_file: String,
}

/// Special cron schedule keywords.
const SCHEDULE_KEYWORDS: &[&str] = &[
    "@reboot",
    "@yearly",
    "@annually",
    "@monthly",
    "@weekly",
    "@daily",
    "@hourly",
];

/// Parse a crontab file.
///
/// Handles both user crontabs (5-field schedule) and system crontabs
/// (6-field with username before command).
///
/// Lines starting with `#` are comments and skipped. Variable assignments
/// (containing `=`) are also skipped. Blank lines are ignored.
pub fn parse_crontab(content: &str) -> Result<Vec<CronJob>, crate::LinuxArtifactError> {
    parse_crontab_with_source(content, "<unknown>")
}

/// Parse a crontab file with an explicit source file path.
pub fn parse_crontab_with_source(
    content: &str,
    source_file: &str,
) -> Result<Vec<CronJob>, crate::LinuxArtifactError> {
    let mut jobs: Vec<CronJob> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Skip environment variable assignments
        if trimmed.contains('=') && !trimmed.contains(' ') && !trimmed.contains('\t') {
            continue;
        }

        let fields: Vec<&str> = trimmed.split_whitespace().collect();
        if fields.is_empty() {
            continue;
        }

        // Check if first field is a schedule keyword
        let is_keyword_schedule = SCHEDULE_KEYWORDS.contains(&fields[0]);

        let (schedule, user, command_start_idx) = if is_keyword_schedule {
            // @keyword [user] command
            if fields.len() >= 3 && !fields[1].contains('/') && !fields[1].starts_with('/') {
                // Check if second field is probably a username (not a path)
                let second_is_user = !fields[1].starts_with('/')
                    && !fields[1].starts_with('.')
                    && fields[1]
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-');
                if second_is_user && fields.len() >= 3 {
                    (fields[0].to_string(), Some(fields[1].to_string()), 2)
                } else {
                    (fields[0].to_string(), None, 1)
                }
            } else {
                (fields[0].to_string(), None, 1)
            }
        } else {
            // 5-field schedule: m h dom mon dow [user] command
            if fields.len() < 6 {
                continue; // incomplete
            }

            let schedule_str = fields[..5].join(" ");

            // Check if field 6 looks like a username or the start of a command
            // For system crontabs with user field, we need at least 7 fields
            if fields.len() >= 7
                && !fields[5].starts_with('/')
                && !fields[5].starts_with('.')
                && fields[5]
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                (schedule_str, Some(fields[5].to_string()), 6)
            } else {
                (schedule_str, None, 5)
            }
        };

        let command = fields[command_start_idx..].join(" ");

        if command.is_empty() {
            continue;
        }

        jobs.push(CronJob {
            schedule,
            user,
            command,
            source_file: source_file.to_string(),
        });
    }

    Ok(jobs)
}

#[cfg(test)]
#[path = "../tests/unit/cron.rs"]
mod tests;
