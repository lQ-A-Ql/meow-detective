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

/// Alias kept for convenience — same as `parse_crontab`.
#[deprecated(note = "Use parse_crontab instead")]
pub fn parse_crontab_file(content: &str) -> Result<Vec<CronJob>, crate::LinuxArtifactError> {
    parse_crontab(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_user_crontab() {
        let input = "\
# Edit this file to introduce tasks to be run by cron.
SHELL=/bin/bash
PATH=/usr/local/sbin:/usr/local/bin:/sbin:/bin:/usr/sbin:/usr/bin

# m h  dom mon dow   command
30 2 * * * /usr/bin/backup.sh
0 */6 * * * /usr/local/bin/cleanup.sh
@daily /usr/bin/logrotate";

        let jobs = parse_crontab(input).expect("should parse crontab");
        assert_eq!(jobs.len(), 3);

        assert_eq!(jobs[0].schedule, "30 2 * * *");
        assert_eq!(jobs[0].user, None);
        assert_eq!(jobs[0].command, "/usr/bin/backup.sh");

        assert_eq!(jobs[1].schedule, "0 */6 * * *");
        assert_eq!(jobs[1].command, "/usr/local/bin/cleanup.sh");

        assert_eq!(jobs[2].schedule, "@daily");
        assert_eq!(jobs[2].command, "/usr/bin/logrotate");
    }

    #[test]
    fn parse_system_crontab_with_user_field() {
        let input = "\
SHELL=/bin/sh
PATH=/usr/local/sbin:/usr/local/bin:/sbin:/bin:/usr/sbin:/usr/bin

17 * * * * root cd / && run-parts --report /etc/cron.hourly
25 6 * * * root test -x /usr/sbin/anacron || ( cd / && run-parts --report /etc/cron.daily )
@reboot root /usr/local/bin/startup.sh";

        let jobs = parse_crontab(input).expect("should parse system crontab");
        assert_eq!(jobs.len(), 3);

        assert_eq!(jobs[0].schedule, "17 * * * *");
        assert_eq!(jobs[0].user.as_deref(), Some("root"));
        assert!(jobs[0].command.contains("run-parts"));

        assert_eq!(jobs[2].schedule, "@reboot");
        assert_eq!(jobs[2].user.as_deref(), Some("root"));
        assert_eq!(jobs[2].command, "/usr/local/bin/startup.sh");
    }

    #[test]
    fn parse_with_source_file() {
        let input = "0 5 * * * /usr/bin/mysql_backup";

        let jobs = parse_crontab_with_source(input, "/etc/cron.d/mysql").expect("should parse");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].source_file, "/etc/cron.d/mysql");
        assert_eq!(jobs[0].command, "/usr/bin/mysql_backup");
    }

    #[test]
    fn parse_keyword_schedules() {
        let input = "\
@yearly /usr/bin/annual-report
@monthly /usr/bin/monthly-cleanup
@weekly /usr/bin/weekly-backup
@daily /usr/bin/daily-sync
@hourly /usr/bin/hourly-check
@reboot /usr/bin/startup";

        let jobs = parse_crontab(input).expect("should parse");
        assert_eq!(jobs.len(), 6);
        assert_eq!(jobs[0].schedule, "@yearly");
        assert_eq!(jobs[5].schedule, "@reboot");
    }

    #[test]
    fn skip_comments_and_blanks() {
        let input =
            "\n# This is a comment\n# Another comment\n0 0 * * * /usr/bin/midnight-task\n\n# End\n";

        let jobs = parse_crontab(input).expect("should parse");
        assert_eq!(jobs.len(), 1);
    }

    #[test]
    fn empty_input() {
        let jobs = parse_crontab("").expect("should parse");
        assert!(jobs.is_empty());
    }

    #[test]
    fn skip_env_assignments() {
        let input = "\
SHELL=/bin/bash
MAILTO=admin@example.com
HOME=/root
0 0 * * * /usr/bin/task";

        let jobs = parse_crontab(input).expect("should parse");
        assert_eq!(jobs.len(), 1);
        // SHELL, MAILTO, HOME lines should be skipped
        assert_eq!(jobs[0].command, "/usr/bin/task");
    }
}
