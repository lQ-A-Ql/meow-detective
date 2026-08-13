use super::common::{
    insert_opt, insert_string_array, truncate, MAX_TEXT_LOG_EVENTS_PER_SOURCE,
    MAX_WEB_ERROR_LOG_EVENTS_PER_SOURCE,
};
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde_json::Value;

pub(in crate::analysis_service::extraction) fn is_nginx_config_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/nginx/nginx.conf")
        || (normalized.contains("/etc/nginx/conf.d/") && normalized.ends_with(".conf"))
        || normalized.contains("/etc/nginx/sites-enabled/")
        // BaoTa panel: per-site vhost configs and the panel-installed nginx.
        || (normalized.contains("/www/server/panel/vhost/nginx/")
            && normalized.ends_with(".conf"))
        || (normalized.contains("/www/server/nginx/conf/") && normalized.ends_with(".conf"))
}

pub(in crate::analysis_service::extraction) fn is_apache_config_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/apache2/apache2.conf")
        || normalized.contains("/etc/apache2/sites-enabled/")
        || normalized.ends_with("/etc/httpd/conf/httpd.conf")
        || (normalized.contains("/etc/httpd/conf.d/") && normalized.ends_with(".conf"))
        // BaoTa panel: per-site vhost configs and the panel-installed apache.
        || (normalized.contains("/www/server/panel/vhost/apache/")
            && normalized.ends_with(".conf"))
        || (normalized.contains("/www/server/apache/conf/") && normalized.ends_with(".conf"))
}

enum WebLogKind {
    Access,
    Error,
}

/// Any `*.log` (including rotated `*.log.N`) under the nginx/apache2/httpd log
/// directories is a web log — vhost names like `other_vhosts_access.log`,
/// `ssl_access_log` or `example.com.access.log` included. Names containing
/// "error" route to the error-log parser, everything else to access. The BaoTa
/// panel centralizes site logs under `/www/wwwlogs/` with the same naming.
fn web_log_kind(normalized: &str) -> Option<WebLogKind> {
    const WEB_LOG_DIRS: &[&str] = &[
        "/var/log/nginx/",
        "/var/log/apache2/",
        "/var/log/httpd/",
        "/www/wwwlogs/",
    ];
    if !WEB_LOG_DIRS.iter().any(|dir| normalized.contains(dir)) {
        return None;
    }
    let name = normalized.rsplit('/').next()?;
    let is_log = name.ends_with(".log")
        || name.contains(".log.")
        || name.ends_with("_log")
        || name.contains("_log.");
    if !is_log {
        return None;
    }
    if name.contains("error") {
        Some(WebLogKind::Error)
    } else {
        Some(WebLogKind::Access)
    }
}

pub(in crate::analysis_service::extraction) fn is_web_access_log_path(normalized: &str) -> bool {
    matches!(web_log_kind(normalized), Some(WebLogKind::Access))
}

pub(in crate::analysis_service::extraction) fn is_web_error_log_path(normalized: &str) -> bool {
    matches!(web_log_kind(normalized), Some(WebLogKind::Error))
}

pub(in crate::analysis_service::extraction) fn is_web_root_script_path(normalized: &str) -> bool {
    (normalized.contains("/var/www/") || normalized.contains("/usr/share/nginx/html/"))
        && [".php", ".phtml", ".jsp", ".jspx", ".asp", ".aspx"]
            .iter()
            .any(|suffix| normalized.ends_with(suffix))
}

pub(super) fn extract_nginx_config(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_nginx_config(&text) {
        Ok(sites) => emit_sites(candidate, sites, "linux.nginx_config", outcome),
        Err(error) => outcome.warnings.push(format!(
            "{} nginx config parse failed: {}",
            candidate.path, error
        )),
    }
}

pub(super) fn extract_apache_config(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_apache_config(&text) {
        Ok(sites) => emit_sites(candidate, sites, "linux.apache_config", outcome),
        Err(error) => outcome.warnings.push(format!(
            "{} apache config parse failed: {}",
            candidate.path, error
        )),
    }
}

pub(super) fn extract_access_log(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_web_access_log(&text) {
        Ok(entries) => {
            if entries.len() > MAX_TEXT_LOG_EVENTS_PER_SOURCE {
                outcome.warnings.push(format!(
                    "{} web access log emitted first {} records only",
                    candidate.path, MAX_TEXT_LOG_EVENTS_PER_SOURCE
                ));
            }
            let findings = artifacts_linux::detect_web_findings(&entries);
            for entry in entries.into_iter().take(MAX_TEXT_LOG_EVENTS_PER_SOURCE) {
                emit_access_log_entry(candidate, entry, outcome);
            }
            for finding in findings {
                emit_finding(candidate, finding, "linux.web_access_log", outcome);
            }
        }
        Err(error) => outcome.warnings.push(format!(
            "{} web access log parse failed: {}",
            candidate.path, error
        )),
    }
}

pub(super) fn extract_error_log(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_web_error_log(&text) {
        Ok(entries) => {
            if entries.len() > MAX_WEB_ERROR_LOG_EVENTS_PER_SOURCE {
                outcome.warnings.push(format!(
                    "{} web error log emitted first {} records only",
                    candidate.path, MAX_WEB_ERROR_LOG_EVENTS_PER_SOURCE
                ));
            }
            for entry in entries
                .into_iter()
                .take(MAX_WEB_ERROR_LOG_EVENTS_PER_SOURCE)
            {
                emit_error_log_entry(candidate, entry, outcome);
            }
        }
        Err(error) => outcome.warnings.push(format!(
            "{} web error log parse failed: {}",
            candidate.path, error
        )),
    }
}

pub(super) fn extract_root_script(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    for finding in artifacts_linux::detect_web_shell(&text, 1) {
        emit_finding(candidate, finding, "linux.web_root_script", outcome);
    }
}

fn emit_sites(
    candidate: &EvidenceCandidate,
    sites: Vec<artifacts_linux::WebSite>,
    parser: &str,
    outcome: &mut ExtractionOutcome,
) {
    if sites.is_empty() {
        outcome.warnings.push(format!(
            "{} contains no auditable nginx/apache site records",
            candidate.path
        ));
    }
    for site in sites {
        let mut attrs = base_attrs(candidate);
        attrs.insert(
            "serverKind".to_string(),
            Value::String(site.server_kind.clone()),
        );
        attrs.insert(
            "siteName".to_string(),
            Value::String(site.site_name.clone()),
        );
        attrs.insert(
            "lineNumber".to_string(),
            Value::Number(site.line_number.into()),
        );
        insert_string_array(&mut attrs, "hostnames", &site.hostnames);
        insert_string_array(&mut attrs, "listen", &site.listen);
        insert_string_array(&mut attrs, "documentRoots", &site.document_roots);
        insert_string_array(&mut attrs, "accessLogs", &site.access_logs);
        insert_string_array(&mut attrs, "errorLogs", &site.error_logs);

        let display_name = site
            .hostnames
            .first()
            .cloned()
            .unwrap_or_else(|| site.site_name.clone());
        outcome.artifacts.push(make_artifact(
            "LinuxWebSite",
            format!("{} site: {}", site.server_kind, display_name),
            format!(
                "listen={}, roots={}",
                site.listen.join(","),
                site.document_roots.join(",")
            ),
            candidate,
            parser,
            attrs,
        ));
    }
}

fn emit_access_log_entry(
    candidate: &EvidenceCandidate,
    entry: artifacts_linux::WebAccessLogEntry,
    outcome: &mut ExtractionOutcome,
) {
    let mut attrs = base_attrs(candidate);
    attrs.insert(
        "clientIp".to_string(),
        Value::String(entry.client_ip.clone()),
    );
    attrs.insert("method".to_string(), Value::String(entry.method.clone()));
    attrs.insert("uri".to_string(), Value::String(entry.uri.clone()));
    attrs.insert(
        "protocol".to_string(),
        Value::String(entry.protocol.clone()),
    );
    attrs.insert("status".to_string(), Value::Number(entry.status.into()));
    attrs.insert(
        "lineNumber".to_string(),
        Value::Number(entry.line_number.into()),
    );
    if let Some(response_bytes) = entry.response_bytes {
        attrs.insert(
            "responseBytes".to_string(),
            Value::Number(response_bytes.into()),
        );
    }
    insert_opt(&mut attrs, "referer", entry.referer.clone());
    insert_opt(&mut attrs, "userAgent", entry.user_agent.clone());
    if let Some(timestamp) = entry.timestamp {
        attrs.insert(
            "timestamp".to_string(),
            Value::String(timestamp.to_rfc3339()),
        );
    }

    outcome.artifacts.push(make_artifact(
        "LinuxWebAccessLog",
        format!("Web access {} {} {}", entry.method, entry.status, entry.uri),
        format!("{} {} {}", entry.client_ip, entry.method, entry.uri),
        candidate,
        "linux.web_access_log",
        attrs,
    ));
}

fn emit_error_log_entry(
    candidate: &EvidenceCandidate,
    entry: artifacts_linux::WebErrorLogEntry,
    outcome: &mut ExtractionOutcome,
) {
    let mut attrs = base_attrs(candidate);
    attrs.insert("message".to_string(), Value::String(entry.message.clone()));
    attrs.insert(
        "lineNumber".to_string(),
        Value::Number(entry.line_number.into()),
    );
    // artifacts-linux keeps the error-log timestamp as a raw string; parse it
    // here so entries join the timeline. Unparseable values stay in attrs as
    // the original text.
    let timestamp = entry
        .timestamp
        .as_deref()
        .and_then(parse_web_error_timestamp);
    match timestamp {
        Some(timestamp) => {
            attrs.insert(
                "timestamp".to_string(),
                Value::String(timestamp.to_rfc3339()),
            );
        }
        None => insert_opt(&mut attrs, "timestamp", entry.timestamp.clone()),
    }
    // artifacts-linux reads severity from the first bracket, which on Apache
    // error lines is the timestamp — severity would always be None. Recover
    // `module:level` from the second bracket here instead.
    let severity = entry
        .severity
        .clone()
        .or_else(|| second_bracketed_severity(&entry.message));
    insert_opt(&mut attrs, "severity", severity);
    outcome.artifacts.push(make_artifact(
        "LinuxWebErrorLog",
        format!("Web error: {}", truncate(&entry.message, 80)),
        entry.message.clone(),
        candidate,
        "linux.web_error_log",
        attrs.clone(),
    ));
    if let Some(timestamp) = timestamp {
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            "linux.web_error_log",
            timestamp,
            format!("Web error: {}", truncate(&entry.message, 80)),
            entry.message,
            attrs,
            "linux.web_error_log",
        ));
    }
}

/// Apache `[Mon Jan 15 10:30:00.123456 2024]` (brackets already stripped) and
/// nginx `2024/01/15 10:30:00` error-log timestamps.
fn parse_web_error_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    if let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%a %b %d %H:%M:%S%.f %Y") {
        return Some(Utc.from_utc_datetime(&naive));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y/%m/%d %H:%M:%S") {
        return Some(Utc.from_utc_datetime(&naive));
    }
    None
}

/// The `[module:level]` bracket following the Apache error-log timestamp.
fn second_bracketed_severity(message: &str) -> Option<String> {
    let first_end = message.find(']')?;
    let rest = message.get(first_end + 1..)?.trim_start();
    let inner = rest.strip_prefix('[')?;
    let end = inner.find(']')?;
    let value = inner[..end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn emit_finding(
    candidate: &EvidenceCandidate,
    finding: artifacts_linux::WebFinding,
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
    insert_opt(&mut attrs, "clientIp", finding.client_ip.clone());
    insert_opt(&mut attrs, "uri", finding.uri.clone());
    if let Some(timestamp) = finding.timestamp {
        attrs.insert(
            "timestamp".to_string(),
            Value::String(timestamp.to_rfc3339()),
        );
    }

    outcome.artifacts.push(make_artifact(
        "LinuxWebFinding",
        format!(
            "Web finding {}: {}",
            finding.severity,
            truncate(&finding.finding_kind, 80)
        ),
        finding.evidence.clone(),
        candidate,
        parser,
        attrs.clone(),
    ));

    if let Some(timestamp) = finding.timestamp {
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            parser,
            timestamp,
            format!("Web finding: {}", finding.finding_kind),
            finding.evidence,
            attrs,
            parser,
        ));
    }
}
