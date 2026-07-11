use super::common::{insert_opt, truncate, MAX_TEXT_LOG_EVENTS_PER_SOURCE};
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use chrono::{DateTime, Utc};
use serde_json::Value;

pub(in crate::analysis_service::extraction) fn is_text_log_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/audit/audit.log")
        || normalized.contains("/var/log/audit/audit.log.")
        || normalized.ends_with("/var/log/syslog")
        || normalized.contains("/var/log/syslog.")
        || normalized.ends_with("/var/log/messages")
        || normalized.contains("/var/log/messages.")
        || normalized.ends_with("/var/log/kern.log")
        || normalized.contains("/var/log/kern.log.")
        || normalized.ends_with("/var/log/cloud-init.log")
        || normalized.contains("/var/log/cloud-init.log.")
}

pub(super) fn extract(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    parser: &str,
    label: &str,
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    let mut emitted = 0usize;
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if emitted >= MAX_TEXT_LOG_EVENTS_PER_SOURCE {
            outcome.warnings.push(format!(
                "{} text log emitted first {} records only",
                candidate.path, MAX_TEXT_LOG_EVENTS_PER_SOURCE
            ));
            break;
        }

        let parsed = parse_syslog_like_line(trimmed);
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
}

struct ParsedTextLogLine {
    timestamp: Option<DateTime<Utc>>,
    hostname: Option<String>,
    syslog_identifier: Option<String>,
    pid: Option<u32>,
    priority: Option<u32>,
    message: String,
}

fn parse_syslog_like_line(line: &str) -> ParsedTextLogLine {
    if let Some(parsed) = parse_rfc3339_log_line(line) {
        return parsed;
    }
    if let Some(parsed) = parse_classic_syslog_line(line) {
        return parsed;
    }
    ParsedTextLogLine {
        timestamp: None,
        hostname: None,
        syslog_identifier: None,
        pid: None,
        priority: audit_priority(line),
        message: line.to_string(),
    }
}

fn parse_rfc3339_log_line(line: &str) -> Option<ParsedTextLogLine> {
    let (head, rest) = line.split_once(' ')?;
    let timestamp = DateTime::parse_from_rfc3339(head)
        .ok()
        .map(|date_time| date_time.with_timezone(&Utc))?;
    let mut parsed = parse_syslog_body(rest);
    parsed.timestamp = Some(timestamp);
    Some(parsed)
}

fn parse_classic_syslog_line(line: &str) -> Option<ParsedTextLogLine> {
    let mut parts = line.splitn(4, ' ');
    let month = parts.next()?;
    let day = parts.next()?;
    let time = parts.next()?;
    let rest = parts.next()?;
    if !is_classic_syslog_timestamp_shape(month, day, time) {
        return None;
    }
    Some(parse_syslog_body(rest))
}

fn parse_syslog_body(body: &str) -> ParsedTextLogLine {
    let (hostname, remainder) = body
        .split_once(' ')
        .map(|(host, rest)| (Some(host.to_string()), rest.trim()))
        .unwrap_or((None, body.trim()));
    let (identifier, pid, message) = parse_identifier_and_message(remainder);
    ParsedTextLogLine {
        timestamp: None,
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

fn is_classic_syslog_timestamp_shape(month: &str, day: &str, time: &str) -> bool {
    matches!(
        month,
        "Jan"
            | "Feb"
            | "Mar"
            | "Apr"
            | "May"
            | "Jun"
            | "Jul"
            | "Aug"
            | "Sep"
            | "Oct"
            | "Nov"
            | "Dec"
    ) && day.parse::<u32>().is_ok()
        && time.split(':').count() == 3
        && time.split(':').all(|part| part.parse::<u32>().is_ok())
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
