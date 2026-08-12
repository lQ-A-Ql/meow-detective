use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// MySQL configuration and log parsing stay side-effect free and read-only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MysqlConfigEntry {
    pub section: Option<String>,
    pub key: String,
    pub value: String,
    pub line_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MysqlLogEntry {
    pub timestamp: Option<DateTime<Utc>>,
    pub severity: Option<String>,
    pub message: String,
    pub thread_id: Option<String>,
    pub line_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MysqlFinding {
    pub finding_kind: String,
    pub severity: String,
    pub confidence: f32,
    pub evidence: String,
    pub line_number: u64,
}

pub fn parse_mysql_config(
    content: &str,
) -> Result<Vec<MysqlConfigEntry>, crate::LinuxArtifactError> {
    let mut entries = Vec::new();
    let mut section: Option<String> = None;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index as u64 + 1;
        let stripped = strip_mysql_comment(raw_line);
        let trimmed = stripped.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let name = trimmed.trim_start_matches('[').trim_end_matches(']').trim();
            section = (!name.is_empty()).then(|| name.to_string());
            continue;
        }

        let (key, value) = parse_key_value(trimmed);
        if key.is_empty() {
            continue;
        }
        entries.push(MysqlConfigEntry {
            section: section.clone(),
            key: key.to_ascii_lowercase().replace('_', "-"),
            value: value.to_string(),
            line_number,
        });
    }

    Ok(entries)
}

pub fn parse_mysql_log(content: &str) -> Result<Vec<MysqlLogEntry>, crate::LinuxArtifactError> {
    Ok(content
        .lines()
        .enumerate()
        .filter_map(|(index, raw_line)| {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                return None;
            }
            Some(parse_mysql_log_line(trimmed, index as u64 + 1))
        })
        .collect())
}

pub fn detect_mysql_config_findings(entries: &[MysqlConfigEntry]) -> Vec<MysqlFinding> {
    let mut findings = Vec::new();
    for entry in entries {
        let value = entry.value.trim().to_ascii_lowercase();
        match entry.key.as_str() {
            "bind-address" if value == "0.0.0.0" || value == "::" || value == "*" => {
                findings.push(MysqlFinding {
                    finding_kind: "bindAddressAny".to_string(),
                    severity: "medium".to_string(),
                    confidence: 0.86,
                    evidence: format!("{}={}", entry.key, entry.value),
                    line_number: entry.line_number,
                });
            }
            "local-infile" if is_enabled(&value) => findings.push(MysqlFinding {
                finding_kind: "localInfileEnabled".to_string(),
                severity: "medium".to_string(),
                confidence: 0.82,
                evidence: format!("{}={}", entry.key, entry.value),
                line_number: entry.line_number,
            }),
            "secure-file-priv" if entry.value.trim().is_empty() => findings.push(MysqlFinding {
                finding_kind: "secureFilePrivEmpty".to_string(),
                severity: "medium".to_string(),
                confidence: 0.78,
                evidence: "secure-file-priv is empty".to_string(),
                line_number: entry.line_number,
            }),
            "general-log" if is_enabled(&value) => findings.push(MysqlFinding {
                finding_kind: "generalLogEnabled".to_string(),
                severity: "low".to_string(),
                confidence: 0.75,
                evidence: format!("{}={}", entry.key, entry.value),
                line_number: entry.line_number,
            }),
            "skip-networking" if !is_enabled(&value) => findings.push(MysqlFinding {
                finding_kind: "networkingEnabled".to_string(),
                severity: "info".to_string(),
                confidence: 0.7,
                evidence: format!("{}={}", entry.key, entry.value),
                line_number: entry.line_number,
            }),
            "init-file" | "plugin-load" | "plugin-load-add" => findings.push(MysqlFinding {
                finding_kind: format!("{}Configured", entry.key.to_ascii_lowercase()),
                severity: "low".to_string(),
                confidence: 0.72,
                evidence: format!("{}={}", entry.key, entry.value),
                line_number: entry.line_number,
            }),
            _ => {}
        }
    }
    findings
}

pub fn detect_mysql_log_findings(entries: &[MysqlLogEntry]) -> Vec<MysqlFinding> {
    let mut findings = Vec::new();
    for entry in entries {
        let lower = entry.message.to_ascii_lowercase();
        let finding_kind = if lower.contains("access denied for user") {
            Some(("accessDenied", "medium", 0.84))
        } else if lower.contains("mysqld got signal") || lower.contains("got signal") {
            Some(("serverCrashSignal", "high", 0.82))
        } else if lower.contains("database was not shutdown normally")
            || lower.contains("crash recovery")
            || lower.contains("starting crash recovery")
        {
            Some(("crashRecovery", "medium", 0.8))
        } else if lower.contains("ready for connections") {
            Some(("serviceStarted", "info", 0.7))
        } else {
            None
        };
        if let Some((kind, severity, confidence)) = finding_kind {
            findings.push(MysqlFinding {
                finding_kind: kind.to_string(),
                severity: severity.to_string(),
                confidence,
                evidence: entry.message.clone(),
                line_number: entry.line_number,
            });
        }
    }
    findings
}

fn parse_mysql_log_line(line: &str, line_number: u64) -> MysqlLogEntry {
    let (timestamp, rest) = parse_mysql_timestamp(line)
        .map(|(ts, tail)| (Some(ts), tail.trim()))
        .unwrap_or((None, line));
    let (thread_id, rest) = parse_thread_id(rest);
    let (severity, message) = parse_bracketed_severity(rest)
        .or_else(|| parse_plain_severity(rest))
        .unwrap_or((None, rest));
    MysqlLogEntry {
        timestamp,
        severity,
        message: message.trim().to_string(),
        thread_id,
        line_number,
    }
}

fn parse_mysql_timestamp(line: &str) -> Option<(DateTime<Utc>, &str)> {
    if let Some((candidate, tail)) = line.split_once(' ') {
        if let Ok(ts) = DateTime::parse_from_rfc3339(candidate) {
            return Some((ts.with_timezone(&Utc), tail));
        }
        if let Ok(ts) = DateTime::parse_from_str(candidate, "%Y-%m-%dT%H:%M:%S%.fZ") {
            return Some((ts.with_timezone(&Utc), tail));
        }
    }
    if line.len() >= 19 {
        let candidate = &line[..19];
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(candidate, "%Y-%m-%d %H:%M:%S") {
            return Some((
                DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc),
                &line[19..],
            ));
        }
    }
    if line.len() >= 15 {
        // Legacy MySQL/MariaDB error log format: "yymmdd HH:MM:SS"
        // (e.g. "240815 10:30:00"). The candidate contains ':' and a space,
        // so the pre-check must allow both.
        let candidate = &line[..15];
        if candidate
            .chars()
            .all(|c| c.is_ascii_digit() || c == ' ' || c == ':')
        {
            // Limitation: the "20" prefix hardcodes the 21st century —
            // two-digit years before 2000 cannot be represented here.
            if let Ok(naive) =
                chrono::NaiveDateTime::parse_from_str(&format!("20{candidate}"), "%Y%m%d %H:%M:%S")
            {
                return Some((
                    DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc),
                    &line[15..],
                ));
            }
        }
    }
    None
}

fn parse_bracketed_severity(rest: &str) -> Option<(Option<String>, &str)> {
    let trimmed = rest.trim_start();
    let after_prefix = trimmed.strip_prefix('[')?;
    let (severity, tail) = after_prefix.split_once(']')?;
    Some((
        Some(severity.trim().to_ascii_lowercase()),
        tail.trim_start(),
    ))
}

fn parse_plain_severity(rest: &str) -> Option<(Option<String>, &str)> {
    let trimmed = rest.trim_start();
    for severity in ["error", "warning", "note", "system", "information"] {
        if trimmed
            .to_ascii_lowercase()
            .starts_with(&format!("{severity} "))
        {
            return Some((Some(severity.to_string()), &trimmed[severity.len()..]));
        }
    }
    None
}

fn parse_thread_id(rest: &str) -> (Option<String>, &str) {
    let trimmed = rest.trim_start();
    let mut parts = trimmed.splitn(2, ' ');
    let first = parts.next().unwrap_or_default();
    if !first.is_empty() && first.chars().all(|c| c.is_ascii_digit()) {
        return (Some(first.to_string()), parts.next().unwrap_or_default());
    }
    (None, trimmed)
}

fn parse_key_value(line: &str) -> (&str, &str) {
    if let Some((key, value)) = line.split_once('=') {
        (
            key.trim(),
            value.trim().trim_matches('"').trim_matches('\''),
        )
    } else {
        (line.trim(), "true")
    }
}

fn strip_mysql_comment(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') || trimmed.starts_with(';') {
        ""
    } else if let Some((head, _)) = line.split_once(" #") {
        head
    } else if let Some((head, _)) = line.split_once(" ;") {
        head
    } else {
        line
    }
}

fn is_enabled(value: &str) -> bool {
    matches!(value.trim(), "1" | "on" | "true" | "yes" | "enabled")
}

#[cfg(test)]
#[path = "../tests/unit/mysql.rs"]
mod tests;
