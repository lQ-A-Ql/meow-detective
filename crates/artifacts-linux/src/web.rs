use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebSite {
    pub server_kind: String,
    pub site_name: String,
    pub hostnames: Vec<String>,
    pub listen: Vec<String>,
    pub document_roots: Vec<String>,
    pub access_logs: Vec<String>,
    pub error_logs: Vec<String>,
    pub line_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebAccessLogEntry {
    pub client_ip: String,
    pub timestamp: Option<DateTime<Utc>>,
    pub method: String,
    pub uri: String,
    pub protocol: String,
    pub status: u16,
    pub response_bytes: Option<u64>,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
    pub line_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebErrorLogEntry {
    pub timestamp: Option<String>,
    pub severity: Option<String>,
    pub message: String,
    pub line_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebFinding {
    pub finding_kind: String,
    pub severity: String,
    pub confidence: f32,
    pub evidence: String,
    pub client_ip: Option<String>,
    pub uri: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    pub line_number: u64,
}

pub fn parse_nginx_config(content: &str) -> Result<Vec<WebSite>, crate::LinuxArtifactError> {
    let mut sites = Vec::new();
    let mut current: Option<WebSite> = None;
    let mut depth = 0i32;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index as u64 + 1;
        let line = strip_inline_comment(raw_line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if current.is_none() && trimmed.starts_with("server") && trimmed.contains('{') {
            current = Some(WebSite {
                server_kind: "nginx".to_string(),
                site_name: format!("nginx server line {line_number}"),
                hostnames: Vec::new(),
                listen: Vec::new(),
                document_roots: Vec::new(),
                access_logs: Vec::new(),
                error_logs: Vec::new(),
                line_number,
            });
            depth = brace_delta(trimmed);
            if depth <= 0 {
                if let Some(site) = current.take() {
                    sites.push(site);
                }
            }
            continue;
        }

        if let Some(site) = current.as_mut() {
            collect_nginx_directive(site, trimmed);
            depth += brace_delta(trimmed);
            if depth <= 0 {
                if let Some(site) = current.take() {
                    sites.push(site);
                }
            }
        }
    }

    if let Some(site) = current {
        sites.push(site);
    }

    Ok(sites)
}

pub fn parse_apache_config(content: &str) -> Result<Vec<WebSite>, crate::LinuxArtifactError> {
    let mut sites = Vec::new();
    let mut current: Option<WebSite> = None;
    let mut global: Option<WebSite> = None;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index as u64 + 1;
        let line = strip_inline_comment(raw_line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("<virtualhost") {
            current = Some(WebSite {
                server_kind: "apache".to_string(),
                site_name: format!("apache vhost line {line_number}"),
                hostnames: Vec::new(),
                listen: virtual_host_listen(trimmed),
                document_roots: Vec::new(),
                access_logs: Vec::new(),
                error_logs: Vec::new(),
                line_number,
            });
            continue;
        }
        if lower.starts_with("</virtualhost") {
            if let Some(site) = current.take() {
                sites.push(site);
            }
            continue;
        }

        if let Some(site) = current.as_mut() {
            collect_apache_directive(site, trimmed);
        } else if is_apache_site_directive(trimmed) {
            let site = global.get_or_insert_with(|| WebSite {
                server_kind: "apache".to_string(),
                site_name: "apache global".to_string(),
                hostnames: Vec::new(),
                listen: Vec::new(),
                document_roots: Vec::new(),
                access_logs: Vec::new(),
                error_logs: Vec::new(),
                line_number,
            });
            collect_apache_directive(site, trimmed);
        }
    }

    if let Some(site) = current {
        sites.push(site);
    }
    if let Some(site) = global {
        sites.push(site);
    }

    Ok(sites)
}

pub fn parse_web_access_log(
    content: &str,
) -> Result<Vec<WebAccessLogEntry>, crate::LinuxArtifactError> {
    let mut entries = Vec::new();
    for (index, line) in content.lines().enumerate() {
        if let Some(entry) = parse_access_log_line(line, index as u64 + 1) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

pub fn parse_web_error_log(
    content: &str,
) -> Result<Vec<WebErrorLogEntry>, crate::LinuxArtifactError> {
    let entries = content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            let severity = extract_bracketed_severity(trimmed);
            Some(WebErrorLogEntry {
                timestamp: extract_error_timestamp(trimmed),
                severity,
                message: trimmed.to_string(),
                line_number: index as u64 + 1,
            })
        })
        .collect();
    Ok(entries)
}

pub fn detect_web_findings(entries: &[WebAccessLogEntry]) -> Vec<WebFinding> {
    let mut findings = Vec::new();
    for entry in entries {
        findings.extend(detect_request_findings(entry));
    }
    findings.extend(detect_bruteforce_findings(entries));
    findings
}

pub fn detect_web_shell(content: &str, line_number_base: u64) -> Vec<WebFinding> {
    let mut findings = Vec::new();
    for (index, line) in content.lines().enumerate() {
        let lower = line.to_ascii_lowercase();
        for needle in [
            "eval(",
            "assert(",
            "base64_decode",
            "shell_exec",
            "system(",
            "passthru(",
            "cmd.exe",
            "powershell",
            "runtime.getruntime",
        ] {
            if lower.contains(needle) {
                findings.push(WebFinding {
                    finding_kind: "webShellCandidate".to_string(),
                    severity: "high".to_string(),
                    confidence: 0.75,
                    evidence: line.trim().chars().take(240).collect(),
                    client_ip: None,
                    uri: None,
                    timestamp: None,
                    line_number: line_number_base + index as u64,
                });
                break;
            }
        }
    }
    findings
}

fn collect_nginx_directive(site: &mut WebSite, line: &str) {
    let line = line.trim_end_matches(';').trim();
    if let Some(rest) = line.strip_prefix("listen ") {
        push_tokens(&mut site.listen, rest);
    } else if let Some(rest) = line.strip_prefix("server_name ") {
        push_tokens(&mut site.hostnames, rest);
    } else if let Some(rest) = line.strip_prefix("root ") {
        push_first_token(&mut site.document_roots, rest);
    } else if let Some(rest) = line.strip_prefix("access_log ") {
        push_first_token(&mut site.access_logs, rest);
    } else if let Some(rest) = line.strip_prefix("error_log ") {
        push_first_token(&mut site.error_logs, rest);
    }
}

fn collect_apache_directive(site: &mut WebSite, line: &str) {
    let mut parts = line.split_whitespace();
    let Some(key) = parts.next() else {
        return;
    };
    let rest = parts.collect::<Vec<_>>().join(" ");
    match key.to_ascii_lowercase().as_str() {
        "servername" => push_first_token(&mut site.hostnames, &rest),
        "serveralias" => push_tokens(&mut site.hostnames, &rest),
        "documentroot" => push_first_token(&mut site.document_roots, &rest),
        "customlog" | "transferlog" => push_first_token(&mut site.access_logs, &rest),
        "errorlog" => push_first_token(&mut site.error_logs, &rest),
        "listen" => push_first_token(&mut site.listen, &rest),
        _ => {}
    }
}

fn parse_access_log_line(line: &str, line_number: u64) -> Option<WebAccessLogEntry> {
    let bracket_start = line.find('[')?;
    let bracket_end = line[bracket_start + 1..].find(']')? + bracket_start + 1;
    let prefix = line[..bracket_start].trim();
    let client_ip = prefix.split_whitespace().next()?.to_string();
    let timestamp_raw = &line[bracket_start + 1..bracket_end];
    let timestamp = DateTime::parse_from_str(timestamp_raw, "%d/%b/%Y:%H:%M:%S %z")
        .ok()
        .map(|dt| dt.with_timezone(&Utc));

    let mut rest = line[bracket_end + 1..].trim();
    let (request, next) = extract_quoted(rest)?;
    rest = next.trim();
    let mut status_parts = rest.splitn(3, ' ');
    let status = status_parts.next()?.parse::<u16>().ok()?;
    let response_bytes = status_parts.next().and_then(|raw| {
        if raw == "-" {
            None
        } else {
            raw.parse::<u64>().ok()
        }
    });
    rest = status_parts.next().unwrap_or_default().trim();

    let (referer, next) = extract_quoted(rest).unwrap_or_else(|| ("-".to_string(), ""));
    let (user_agent, _) = extract_quoted(next.trim()).unwrap_or_else(|| ("-".to_string(), ""));
    let request_parts = request.split_whitespace().collect::<Vec<_>>();
    if request_parts.len() < 3 {
        return None;
    }

    Some(WebAccessLogEntry {
        client_ip,
        timestamp,
        method: request_parts[0].to_string(),
        uri: request_parts[1].to_string(),
        protocol: request_parts[2].to_string(),
        status,
        response_bytes,
        referer: dash_to_none(referer),
        user_agent: dash_to_none(user_agent),
        line_number,
    })
}

fn detect_request_findings(entry: &WebAccessLogEntry) -> Vec<WebFinding> {
    let mut findings = Vec::new();
    let referer = entry.referer.as_deref().unwrap_or_default();
    let user_agent = entry.user_agent.as_deref().unwrap_or_default();
    let inspected = format!("{} {referer} {user_agent}", entry.uri).to_ascii_lowercase();
    for (kind, severity, confidence, needles) in [
        (
            "sqlInjection",
            "high",
            0.9,
            &[
                "union%20select",
                "union select",
                " or 1=1",
                "%27%20or%20",
                "information_schema",
                "sleep(",
                "benchmark(",
            ][..],
        ),
        (
            "localFileInclusion",
            "high",
            0.9,
            &["../", "..%2f", "/etc/passwd", "win.ini", "php://filter"][..],
        ),
        (
            "crossSiteScripting",
            "medium",
            0.75,
            &["<script", "%3cscript", "javascript:", "onerror=", "onload="][..],
        ),
        (
            "scannerFingerprint",
            "medium",
            0.8,
            &[
                "sqlmap",
                "nikto",
                "dirbuster",
                "gobuster",
                "wfuzz",
                "acunetix",
            ][..],
        ),
    ] {
        if needles.iter().any(|needle| inspected.contains(needle)) {
            findings.push(WebFinding {
                finding_kind: kind.to_string(),
                severity: severity.to_string(),
                confidence,
                evidence: format!("{} {} ua={user_agent}", entry.method, entry.uri),
                client_ip: Some(entry.client_ip.clone()),
                uri: Some(entry.uri.clone()),
                timestamp: entry.timestamp,
                line_number: entry.line_number,
            });
        }
    }
    findings
}

fn detect_bruteforce_findings(entries: &[WebAccessLogEntry]) -> Vec<WebFinding> {
    let mut counts: HashMap<(&str, i64), u32> = HashMap::new();
    for entry in entries {
        if !entry.method.eq_ignore_ascii_case("post") || !looks_like_login_uri(&entry.uri) {
            continue;
        }
        let Some(ts) = entry.timestamp else {
            continue;
        };
        let bucket = ts.timestamp() / 300;
        *counts.entry((&entry.client_ip, bucket)).or_default() += 1;
    }
    counts
        .into_iter()
        .filter_map(|((client_ip, bucket), count)| {
            if count < 50 {
                return None;
            }
            Some(WebFinding {
                finding_kind: "bruteforce".to_string(),
                severity: "high".to_string(),
                confidence: 0.8,
                evidence: format!("{count} POST login requests in five-minute bucket {bucket}"),
                client_ip: Some(client_ip.to_string()),
                uri: None,
                timestamp: None,
                line_number: 0,
            })
        })
        .collect()
}

fn looks_like_login_uri(uri: &str) -> bool {
    let lower = uri.to_ascii_lowercase();
    lower.contains("login")
        || lower.contains("signin")
        || lower.contains("wp-login")
        || lower.contains("admin")
}

fn is_apache_site_directive(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("servername ")
        || lower.starts_with("serveralias ")
        || lower.starts_with("documentroot ")
        || lower.starts_with("customlog ")
        || lower.starts_with("transferlog ")
        || lower.starts_with("errorlog ")
        || lower.starts_with("listen ")
}

fn virtual_host_listen(line: &str) -> Vec<String> {
    let inner = line
        .trim_start_matches("<VirtualHost")
        .trim_end_matches('>')
        .trim();
    if inner.is_empty() {
        Vec::new()
    } else {
        inner
            .split_whitespace()
            .map(|value| value.to_string())
            .collect()
    }
}

fn extract_bracketed_severity(line: &str) -> Option<String> {
    let start = line.find('[')?;
    let end = line[start + 1..].find(']')? + start + 1;
    let value = &line[start + 1..end];
    if value.contains("error") || value.contains("warn") || value.contains("notice") {
        Some(value.to_string())
    } else {
        None
    }
}

fn extract_error_timestamp(line: &str) -> Option<String> {
    if line.starts_with('[') {
        return line
            .find(']')
            .map(|end| line[..=end].trim_matches(['[', ']']).to_string());
    }
    if line.len() >= 19 && line.as_bytes().get(4) == Some(&b'/') {
        return Some(line[..19].to_string());
    }
    None
}

fn strip_inline_comment(line: &str) -> String {
    line.split('#').next().unwrap_or_default().to_string()
}

fn brace_delta(line: &str) -> i32 {
    line.chars().filter(|ch| *ch == '{').count() as i32
        - line.chars().filter(|ch| *ch == '}').count() as i32
}

fn push_tokens(target: &mut Vec<String>, value: &str) {
    for token in value.split_whitespace() {
        push_clean(target, token);
    }
}

fn push_first_token(target: &mut Vec<String>, value: &str) {
    if let Some(token) = value.split_whitespace().next() {
        push_clean(target, token);
    }
}

fn push_clean(target: &mut Vec<String>, token: &str) {
    let cleaned = token
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_end_matches(';');
    if !cleaned.is_empty() && cleaned != "off" && !target.iter().any(|value| value == cleaned) {
        target.push(cleaned.to_string());
    }
}

fn extract_quoted(input: &str) -> Option<(String, &str)> {
    let start = input.find('"')?;
    let rest = &input[start + 1..];
    let end = rest.find('"')?;
    Some((rest[..end].to_string(), &rest[end + 1..]))
}

fn dash_to_none(value: String) -> Option<String> {
    if value == "-" || value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
#[path = "../tests/unit/web.rs"]
mod tests;
