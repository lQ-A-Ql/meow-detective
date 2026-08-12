use super::common::{insert_opt, truncate, MAX_TEXT_LOG_EVENTS_PER_SOURCE};
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use serde_json::Value;

pub(in crate::analysis_service::extraction) fn is_text_log_path(normalized: &str) -> bool {
    [
        "/var/log/audit/audit.log",
        "/var/log/syslog",
        "/var/log/messages",
        "/var/log/kern.log",
        "/var/log/cloud-init.log",
        "/var/log/cron",
        "/var/log/daemon.log",
        "/var/log/mail.log",
        "/var/log/maillog",
        "/var/log/ufw.log",
        "/var/log/fail2ban.log",
    ]
    .iter()
    .any(|base| normalized.ends_with(base) || normalized.contains(&format!("{base}.")))
}

pub(super) fn extract(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    parser: &str,
    label: &str,
    outcome: &mut ExtractionOutcome,
) {
    extract_with_filter(candidate, bytes, parser, label, &|_| false, outcome);
}

/// Extract text-log lines, skipping lines already consumed by a structured
/// channel (e.g. sudo lines in auth.log) so each line is emitted exactly once.
pub(super) fn extract_with_filter(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    parser: &str,
    label: &str,
    skip: &dyn Fn(&str) -> bool,
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    let mut emitted = 0usize;
    let mut unanchored = 0usize;
    let mut context = TimestampContext {
        reference: candidate.modified_at,
        fallback_year: None,
    };
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || skip(trimmed) {
            continue;
        }
        if emitted >= MAX_TEXT_LOG_EVENTS_PER_SOURCE {
            outcome.warnings.push(format!(
                "{} text log emitted first {} records only",
                candidate.path, MAX_TEXT_LOG_EVENTS_PER_SOURCE
            ));
            break;
        }

        let parsed = parse_syslog_like_line(trimmed, &mut context);
        let mut attrs = base_attrs(candidate);
        attrs.insert("message".to_string(), Value::String(parsed.message.clone()));
        attrs.insert(
            "lineNumber".to_string(),
            Value::Number((line_number as u64 + 1).into()),
        );
        attrs.insert("logKind".to_string(), Value::String(label.to_string()));
        insert_opt(&mut attrs, "hostname", parsed.hostname.clone());
        insert_opt(
            &mut attrs,
            "syslogIdentifier",
            parsed.syslog_identifier.clone(),
        );
        if let Some(pid) = parsed.pid {
            attrs.insert("pid".to_string(), Value::Number(pid.into()));
        }
        if let Some(priority) = parsed.priority {
            attrs.insert("priority".to_string(), Value::Number(priority.into()));
        }
        if let Some(timestamp) = parsed.timestamp {
            attrs.insert(
                "timestamp".to_string(),
                Value::String(timestamp.to_rfc3339()),
            );
        }
        if let Some(unanchored_date) = parsed.unanchored_date.clone() {
            unanchored += 1;
            attrs.insert("logDate".to_string(), Value::String(unanchored_date));
        }

        outcome.artifacts.push(make_artifact(
            "LinuxJournal",
            format!("{} log: {}", label, truncate(&parsed.message, 80)),
            parsed.message.clone(),
            candidate,
            parser,
            attrs.clone(),
        ));

        if let Some(timestamp) = parsed.timestamp {
            outcome.timeline_events.push(make_timeline_event(
                &candidate.file_id,
                parser,
                timestamp,
                format!("{} log: {}", label, truncate(&parsed.message, 80)),
                parsed.message,
                attrs,
                parser,
            ));
        }
        emitted += 1;
    }
    if unanchored > 0 {
        outcome.warnings.push(format!(
            "{} {} classic syslog lines have no year anchor (file mtime and RFC3339 lines unavailable); timestamp left empty",
            candidate.path, unanchored
        ));
    }
}

/// Year anchors for classic syslog timestamps that carry no year.
struct TimestampContext {
    /// Candidate file mtime: preferred reference for the year heuristic.
    reference: Option<DateTime<Utc>>,
    /// Year of an RFC3339 line already seen in the same file.
    fallback_year: Option<i32>,
}

struct ParsedTextLogLine {
    timestamp: Option<DateTime<Utc>>,
    /// Month/day/time kept when no year anchor exists (e.g. "Mar 5 12:00:00").
    unanchored_date: Option<String>,
    hostname: Option<String>,
    syslog_identifier: Option<String>,
    pid: Option<u32>,
    priority: Option<u32>,
    message: String,
}

fn parse_syslog_like_line(line: &str, context: &mut TimestampContext) -> ParsedTextLogLine {
    if let Some(parsed) = parse_rfc3339_log_line(line, context) {
        return parsed;
    }
    if let Some(parsed) = parse_classic_syslog_line(line, context) {
        return parsed;
    }
    ParsedTextLogLine {
        timestamp: parse_audit_epoch(line),
        unanchored_date: None,
        hostname: None,
        syslog_identifier: None,
        pid: None,
        priority: audit_priority(line),
        message: line.to_string(),
    }
}

fn parse_rfc3339_log_line(line: &str, context: &mut TimestampContext) -> Option<ParsedTextLogLine> {
    let (head, rest) = line.split_once(' ')?;
    let timestamp = DateTime::parse_from_rfc3339(head)
        .ok()
        .map(|date_time| date_time.with_timezone(&Utc))?;
    context.fallback_year = Some(timestamp.year());
    let mut parsed = parse_syslog_body(rest);
    parsed.timestamp = Some(timestamp);
    Some(parsed)
}

fn parse_classic_syslog_line(line: &str, context: &TimestampContext) -> Option<ParsedTextLogLine> {
    let (month, day, time, rest) = split_classic_head(line)?;
    let month_number = classic_month_number(month)?;
    let day_number = day
        .parse::<u32>()
        .ok()
        .filter(|day| (1..=31).contains(day))?;
    let time = NaiveTime::parse_from_str(time, "%H:%M:%S").ok()?;
    let mut parsed = parse_syslog_body(rest);
    match resolve_classic_timestamp(month_number, day_number, time, context) {
        Some(timestamp) => parsed.timestamp = Some(timestamp),
        None => parsed.unanchored_date = Some(format!("{month} {day} {time}")),
    }
    Some(parsed)
}

/// Split `Mar  5 12:00:00 host svc[pid]: msg` into month/day/time tokens and
/// the remaining body, tolerating runs of whitespace between fields.
fn split_classic_head(line: &str) -> Option<(&str, &str, &str, &str)> {
    let mut position = 0usize;
    let mut tokens = [""; 3];
    for token in &mut tokens {
        let rest = line.get(position..)?;
        let leading = rest.len() - rest.trim_start().len();
        let start = position + leading;
        let tail = line.get(start..)?;
        let width = tail.find(char::is_whitespace).unwrap_or(tail.len());
        if width == 0 {
            return None;
        }
        *token = &tail[..width];
        position = start + width;
    }
    let rest = line.get(position..)?.trim();
    if rest.is_empty() {
        return None;
    }
    Some((tokens[0], tokens[1], tokens[2], rest))
}

fn classic_month_number(month: &str) -> Option<u32> {
    match month {
        "Jan" => Some(1),
        "Feb" => Some(2),
        "Mar" => Some(3),
        "Apr" => Some(4),
        "May" => Some(5),
        "Jun" => Some(6),
        "Jul" => Some(7),
        "Aug" => Some(8),
        "Sep" => Some(9),
        "Oct" => Some(10),
        "Nov" => Some(11),
        "Dec" => Some(12),
        _ => None,
    }
}

/// Anchor a year-less classic syslog timestamp: prefer the candidate file
/// mtime year, rolling back one year when the result would be later than the
/// reference; otherwise reuse the year of RFC3339 lines seen in the same file.
fn resolve_classic_timestamp(
    month: u32,
    day: u32,
    time: NaiveTime,
    context: &TimestampContext,
) -> Option<DateTime<Utc>> {
    let year = context
        .reference
        .map(|reference| reference.year())
        .or(context.fallback_year)?;
    let naive = NaiveDateTime::new(NaiveDate::from_ymd_opt(year, month, day)?, time);
    let mut timestamp = Utc.from_utc_datetime(&naive);
    if let Some(reference) = context.reference {
        if timestamp > reference {
            let rolled = NaiveDate::from_ymd_opt(year - 1, month, day)?;
            timestamp = Utc.from_utc_datetime(&NaiveDateTime::new(rolled, time));
        }
    }
    Some(timestamp)
}

/// Parse the epoch inside audit.log's `msg=audit(1699999999.123:456)` stamp.
fn parse_audit_epoch(line: &str) -> Option<DateTime<Utc>> {
    let start = line.find("audit(")? + "audit(".len();
    let stamp = line.get(start..)?.split(':').next()?;
    let (seconds, fraction) = stamp.split_once('.').unwrap_or((stamp, "0"));
    let seconds = seconds.parse::<i64>().ok()?;
    let millis = fraction
        .get(..3)
        .and_then(|digits| digits.parse::<i64>().ok())
        .unwrap_or(0);
    DateTime::from_timestamp_millis(seconds * 1_000 + millis)
}

fn parse_syslog_body(body: &str) -> ParsedTextLogLine {
    let (hostname, remainder) = body
        .split_once(' ')
        .map(|(host, rest)| (Some(host.to_string()), rest.trim()))
        .unwrap_or((None, body.trim()));
    let (identifier, pid, message) = parse_identifier_and_message(remainder);
    ParsedTextLogLine {
        timestamp: None,
        unanchored_date: None,
        hostname,
        syslog_identifier: identifier,
        pid,
        priority: audit_priority(message).or_else(|| syslog_priority(message)),
        message: message.to_string(),
    }
}

fn parse_identifier_and_message(input: &str) -> (Option<String>, Option<u32>, &str) {
    let Some((prefix, message)) = input.split_once(':') else {
        return (None, None, input);
    };
    let prefix = prefix.trim();
    if prefix.is_empty() || prefix.contains(' ') {
        return (None, None, input);
    }
    if let Some((identifier, pid_text)) = prefix.split_once('[') {
        let pid = pid_text.trim_end_matches(']').parse::<u32>().ok();
        return (Some(identifier.to_string()), pid, message.trim());
    }
    (Some(prefix.to_string()), None, message.trim())
}

fn audit_priority(line: &str) -> Option<u32> {
    if line.contains("type=SYSCALL") || line.contains("type=EXECVE") {
        Some(5)
    } else if line.contains("type=USER_AUTH") || line.contains("type=USER_LOGIN") {
        Some(6)
    } else {
        None
    }
}

fn syslog_priority(message: &str) -> Option<u32> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("error") || lower.contains("fail") || lower.contains("denied") {
        Some(3)
    } else if lower.contains("warn") {
        Some(4)
    } else {
        None
    }
}
