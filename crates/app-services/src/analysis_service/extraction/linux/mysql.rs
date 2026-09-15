use super::common::{insert_opt, truncate, warn_on_parse_gap, MAX_MYSQL_LOG_EVENTS_PER_SOURCE};
use super::timezone::{LinuxLogTimeContext, UNVERIFIED_TIMEZONE_LABEL};
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::Value;

pub(in crate::analysis_service::extraction) fn is_mysql_config_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/my.cnf")
        || normalized.ends_with("/etc/mysql/my.cnf")
        || (normalized.contains("/etc/mysql/mysql.conf.d/") && normalized.ends_with(".cnf"))
        || (normalized.contains("/etc/mysql/mariadb.conf.d/") && normalized.ends_with(".cnf"))
        || (normalized.contains("/etc/mysql/conf.d/") && normalized.ends_with(".cnf"))
        || (normalized.contains("/etc/my.cnf.d/") && normalized.ends_with(".cnf"))
}

pub(in crate::analysis_service::extraction) fn is_mysql_log_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/mysql/error.log")
        || normalized.contains("/var/log/mysql/error.log.")
        || normalized.ends_with("/var/log/mysql/mysql.log")
        || normalized.contains("/var/log/mysql/mysql.log.")
        || normalized.ends_with("/var/log/mysql/mysql-slow.log")
        || normalized.contains("/var/log/mysql/mysql-slow.log.")
        || normalized.ends_with("/var/log/mysql/slow.log")
        || normalized.contains("/var/log/mysql/slow.log.")
        || normalized.ends_with("/var/log/mariadb/mariadb.log")
        || normalized.contains("/var/log/mariadb/mariadb.log.")
        || normalized.ends_with("/var/log/mysqld.log")
        || normalized.contains("/var/log/mysqld.log.")
}

pub(super) fn extract_config(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_mysql_config(&text) {
        Ok(entries) => {
            let findings = artifacts_linux::detect_mysql_config_findings(&entries);
            if entries.is_empty() {
                outcome.warnings.push(format!(
                    "{} contains no auditable MySQL/MariaDB config entries",
                    candidate.path
                ));
            }
            for entry in entries {
                emit_config_entry(candidate, entry, outcome);
            }
            for finding in findings {
                emit_finding(candidate, finding, "linux.mysql_config", outcome);
            }
        }
        Err(error) => outcome.warnings.push(format!(
            "{} MySQL config parse failed: {}",
            candidate.path, error
        )),
    }
}

pub(super) fn extract_log(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
    log_time: &LinuxLogTimeContext,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_mysql_log_with_stats(&text) {
        Ok((entries, stats)) => {
            warn_on_parse_gap(
                candidate,
                "MySQL log",
                stats.total_lines,
                stats.parsed_lines,
                &mut outcome.warnings,
            );
            if entries.len() > MAX_MYSQL_LOG_EVENTS_PER_SOURCE {
                outcome.warnings.push(format!(
                    "{} MySQL log emitted first {} records only",
                    candidate.path, MAX_MYSQL_LOG_EVENTS_PER_SOURCE
                ));
            }
            let findings = artifacts_linux::detect_mysql_log_findings(&entries);
            let mut unverified = 0usize;
            for entry in entries.into_iter().take(MAX_MYSQL_LOG_EVENTS_PER_SOURCE) {
                if emit_log_entry(candidate, entry, outcome, log_time) {
                    unverified += 1;
                }
            }
            if unverified > 0 {
                outcome.warnings.push(format!(
                    "{} timezone not determined; {unverified} MySQL log timestamps kept as raw text ({UNVERIFIED_TIMEZONE_LABEL})",
                    candidate.path
                ));
            }
            for finding in findings {
                emit_finding(candidate, finding, "linux.mysql_log", outcome);
            }
        }
        Err(error) => outcome.warnings.push(format!(
            "{} MySQL log parse failed: {}",
            candidate.path, error
        )),
    }
}

fn emit_config_entry(
    candidate: &EvidenceCandidate,
    entry: artifacts_linux::MysqlConfigEntry,
    outcome: &mut ExtractionOutcome,
) {
    let mut attrs = base_attrs(candidate);
    insert_opt(&mut attrs, "section", entry.section.clone());
    attrs.insert("key".to_string(), Value::String(entry.key.clone()));
    attrs.insert("value".to_string(), Value::String(entry.value.clone()));
    attrs.insert(
        "lineNumber".to_string(),
        Value::Number(entry.line_number.into()),
    );
    outcome.artifacts.push(make_artifact(
        "LinuxMysqlConfig",
        format!("MySQL config {}", entry.key),
        format!("{}={}", entry.key, entry.value),
        candidate,
        "linux.mysql_config",
        attrs,
    ));
}

fn emit_log_entry(
    candidate: &EvidenceCandidate,
    entry: artifacts_linux::MysqlLogEntry,
    outcome: &mut ExtractionOutcome,
    log_time: &LinuxLogTimeContext,
) -> bool {
    let mut attrs = base_attrs(candidate);
    attrs.insert("message".to_string(), Value::String(entry.message.clone()));
    attrs.insert(
        "lineNumber".to_string(),
        Value::Number(entry.line_number.into()),
    );
    insert_opt(&mut attrs, "severity", entry.severity.clone());
    insert_opt(&mut attrs, "threadId", entry.thread_id.clone());
    // artifacts-linux keeps the raw timestamp token. ISO 8601 tokens carry
    // their own offset (MySQL 8 `log_timestamps`, both UTC and SYSTEM) and
    // are already absolute; the older naive formats are server-local and
    // convert with the inferred host zone. With no zone determined the raw
    // text stays with an unverified-timezone marker instead of fake UTC.
    let parsed = entry
        .timestamp
        .as_deref()
        .and_then(parse_mysql_log_timestamp);
    let local_timestamp = matches!(parsed, Some(MysqlLogTimestamp::Local(_)));
    let (timestamp, tz_label) = match parsed {
        Some(MysqlLogTimestamp::Absolute(timestamp)) => (Some(timestamp), None),
        Some(MysqlLogTimestamp::Local(naive)) if !log_time.assumed_utc() => {
            match log_time.clock().local_to_utc(naive) {
                Some(timestamp) => (Some(timestamp), Some(log_time.tz_label().to_string())),
                None => (None, Some(UNVERIFIED_TIMEZONE_LABEL.to_string())),
            }
        }
        Some(MysqlLogTimestamp::Local(_)) => (None, Some(UNVERIFIED_TIMEZONE_LABEL.to_string())),
        None => (None, None),
    };
    match timestamp {
        Some(timestamp) => {
            attrs.insert(
                "timestamp".to_string(),
                Value::String(timestamp.to_rfc3339()),
            );
        }
        None => insert_opt(&mut attrs, "timestamp", entry.timestamp.clone()),
    }
    if let Some(label) = tz_label {
        attrs.insert("tzAssumed".to_string(), Value::String(label));
    }
    outcome.artifacts.push(make_artifact(
        "LinuxMysqlLogEntry",
        format!("MySQL log: {}", truncate(&entry.message, 80)),
        entry.message.clone(),
        candidate,
        "linux.mysql_log",
        attrs.clone(),
    ));
    if let Some(timestamp) = timestamp {
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            "linux.mysql_log",
            timestamp,
            "MySQL service log".to_string(),
            entry.message,
            attrs,
            "linux.mysql_log",
        ));
    }
    local_timestamp && timestamp.is_none()
}

/// MySQL error/general log timestamps: ISO 8601 with offset (absolute) or
/// server-local naive text.
#[derive(Debug)]
enum MysqlLogTimestamp {
    Absolute(DateTime<Utc>),
    Local(NaiveDateTime),
}

fn parse_mysql_log_timestamp(raw: &str) -> Option<MysqlLogTimestamp> {
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(raw) {
        return Some(MysqlLogTimestamp::Absolute(timestamp.with_timezone(&Utc)));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        return Some(MysqlLogTimestamp::Local(naive));
    }
    parse_legacy_mysql_timestamp(raw).map(MysqlLogTimestamp::Local)
}

/// Legacy MySQL <=5.6 / early MariaDB error-log dates are `yymmdd HH:MM:SS`
/// (15 bytes). That format shipped from 2003 until MySQL 5.7 replaced it
/// with ISO 8601, so every authentic two-digit year falls inside 2000-2099;
/// the "20" prefix is the format's explicit century boundary.
fn parse_legacy_mysql_timestamp(raw: &str) -> Option<NaiveDateTime> {
    let legacy_shape = raw.len() == 15
        && raw
            .chars()
            .all(|c| c.is_ascii_digit() || c == ' ' || c == ':');
    if !legacy_shape {
        return None;
    }
    NaiveDateTime::parse_from_str(&format!("20{raw}"), "%Y%m%d %H:%M:%S").ok()
}

fn emit_finding(
    candidate: &EvidenceCandidate,
    finding: artifacts_linux::MysqlFinding,
    parser: &str,
    outcome: &mut ExtractionOutcome,
) {
    let mut attrs = base_attrs(candidate);
    attrs.insert(
        "findingKind".to_string(),
        Value::String(finding.finding_kind.clone()),
    );
    attrs.insert(
        "severity".to_string(),
        Value::String(finding.severity.clone()),
    );
    attrs.insert(
        "confidence".to_string(),
        Value::Number(
            serde_json::Number::from_f64(finding.confidence as f64)
                .unwrap_or_else(|| serde_json::Number::from(0)),
        ),
    );
    attrs.insert(
        "evidence".to_string(),
        Value::String(finding.evidence.clone()),
    );
    attrs.insert(
        "lineNumber".to_string(),
        Value::Number(finding.line_number.into()),
    );

    outcome.artifacts.push(make_artifact(
        "LinuxMysqlFinding",
        format!(
            "MySQL finding {}: {}",
            finding.severity,
            truncate(&finding.finding_kind, 80)
        ),
        finding.evidence,
        candidate,
        parser,
        attrs,
    ));
}

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/linux/mysql.rs"]
mod tests;
