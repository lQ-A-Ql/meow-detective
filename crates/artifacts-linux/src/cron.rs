//! Crontab parser.
//!
//! Parses crontab files in the standard vixie-cron format, including:
//! - `/var/spool/cron/crontabs/<user>` (user crontabs, no user field)
//! - `/etc/crontab` (system crontab, which includes a user field)
//! - `/etc/cron.d/*` (drop-in cron files, also with a user field)
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
//!
//! Known limitation: a `%` inside the command acts as a newline in real
//! cron (later lines become stdin); this parser keeps the raw line and does
//! not expand `%` semantics.

use serde::{Deserialize, Serialize};

/// A cron job entry from a crontab file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CronJob {
    /// The cron schedule expression (e.g. "30 2 * * *" or "@daily")
    pub schedule: String,
    /// The user the job runs as (system crontabs only; always `None` for
    /// user crontabs)
    pub user: Option<String>,
    /// The command to execute
    pub command: String,
    /// Source file path (for identification)
    pub source_file: String,
}

/// Whether the parsed file follows system- or user-crontab syntax.
///
/// System crontabs (`/etc/crontab`, `/etc/cron.d/*`) carry a username field
/// between the schedule and the command; user crontabs
/// (`/var/spool/cron/crontabs/<user>`) do not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrontabKind {
    /// `/etc/crontab`, `/etc/cron.d/*`: `m h dom mon dow user command`
    System,
    /// `/var/spool/cron/crontabs/<user>`: `m h dom mon dow command`
    User,
}

/// Special cron schedule keywords.
const SCHEDULE_KEYWORDS: &[&str] = &[
    "@reboot",
    "@yearly",
    "@annually",
    "@monthly",
    "@weekly",
    "@daily",
    "@midnight",
    "@hourly",
];

/// Parse a crontab file.
///
/// Deprecated semantics: the file is assumed to follow [`CrontabKind::System`]
/// syntax, which mis-parses user crontabs whose command starts with a bare
/// word (e.g. `0 0 * * * echo hello`). Prefer
/// [`parse_crontab_with_source_and_kind`] with an explicit kind. Retained for
/// backward compatibility.
///
/// Lines starting with `#` are comments and skipped. Variable assignments
/// (containing `=`) are also skipped. Blank lines are ignored.
pub fn parse_crontab(content: &str) -> Result<Vec<CronJob>, crate::LinuxArtifactError> {
    parse_crontab_impl(content, "<unknown>", CrontabKind::System)
}

/// Parse a crontab file with an explicit source file path and kind.
pub fn parse_crontab_with_source_and_kind(
    content: &str,
    source_file: &str,
    kind: CrontabKind,
) -> Result<Vec<CronJob>, crate::LinuxArtifactError> {
    parse_crontab_impl(content, source_file, kind)
}

fn parse_crontab_impl(
    content: &str,
    source_file: &str,
    kind: CrontabKind,
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

        let Some((schedule, user, command_start_idx)) = split_entry_fields(&fields, kind) else {
            continue;
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

/// Split a non-comment crontab line into (schedule, user, command start).
///
/// Returns `None` for incomplete lines and for lines whose schedule fields
/// are not cron syntax at all (e.g. shell lines from a misrouted
/// `/etc/cron.daily/*` script).
fn split_entry_fields(
    fields: &[&str],
    kind: CrontabKind,
) -> Option<(String, Option<String>, usize)> {
    // Check if first field is a schedule keyword
    if SCHEDULE_KEYWORDS.contains(&fields[0]) {
        // @keyword [user] command  (user only in system crontabs)
        if kind == CrontabKind::System && fields.len() >= 3 && looks_like_username(fields[1]) {
            return Some((fields[0].to_string(), Some(fields[1].to_string()), 2));
        }
        if fields.len() >= 2 {
            return Some((fields[0].to_string(), None, 1));
        }
        return None;
    }

    // 5-field schedule: m h dom mon dow [user] command
    if fields.len() < 6 {
        return None;
    }
    if !fields[..5].iter().all(|field| is_schedule_field(field)) {
        return None;
    }

    let schedule_str = fields[..5].join(" ");
    if kind == CrontabKind::System && fields.len() >= 7 && looks_like_username(fields[5]) {
        return Some((schedule_str, Some(fields[5].to_string()), 6));
    }
    Some((schedule_str, None, 5))
}

/// A cron schedule field only contains digits and the `,` `*` `/` `-`
/// list/range/step operators. Anything else (shell words, `$VARS`, quotes,
/// `[` brackets) means the line is not a crontab entry.
fn is_schedule_field(field: &str) -> bool {
    !field.is_empty()
        && field
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b',' | b'*' | b'/' | b'-'))
}

/// A username field is a bare account name — never a path or option.
fn looks_like_username(field: &str) -> bool {
    !field.starts_with('/')
        && !field.starts_with('.')
        && !field.starts_with('-')
        && field
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
#[path = "../tests/unit/cron.rs"]
mod tests;
