use super::{WebAccessLogEntry, WebFinding};
use std::collections::HashMap;

pub fn detect_web_findings(entries: &[WebAccessLogEntry]) -> Vec<WebFinding> {
    let mut findings = entries
        .iter()
        .flat_map(detect_request_findings)
        .collect::<Vec<_>>();
    findings.extend(detect_bruteforce_findings(entries));
    findings
}

pub fn detect_web_shell(content: &str, line_number_base: u64) -> Vec<WebFinding> {
    content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let lower = line.to_ascii_lowercase();
            let matched = WEB_SHELL_MARKERS
                .iter()
                .any(|needle| lower.contains(needle));
            matched.then(|| WebFinding {
                finding_kind: "webShellCandidate".to_string(),
                severity: "high".to_string(),
                confidence: 0.75,
                evidence: line.trim().chars().take(240).collect(),
                client_ip: None,
                uri: None,
                timestamp: None,
                line_number: line_number_base + index as u64,
            })
        })
        .collect()
}

const WEB_SHELL_MARKERS: [&str; 9] = [
    "eval(",
    "assert(",
    "base64_decode",
    "shell_exec",
    "system(",
    "passthru(",
    "cmd.exe",
    "powershell",
    "runtime.getruntime",
];

fn detect_request_findings(entry: &WebAccessLogEntry) -> Vec<WebFinding> {
    let referer = entry.referer.as_deref().unwrap_or_default();
    let user_agent = entry.user_agent.as_deref().unwrap_or_default();
    let inspected = format!("{} {referer} {user_agent}", entry.uri).to_ascii_lowercase();
    REQUEST_RULES
        .iter()
        .filter(|rule| rule.needles.iter().any(|needle| inspected.contains(needle)))
        .map(|rule| WebFinding {
            finding_kind: rule.kind.to_string(),
            severity: rule.severity.to_string(),
            confidence: rule.confidence,
            evidence: format!("{} {} ua={user_agent}", entry.method, entry.uri),
            client_ip: Some(entry.client_ip.clone()),
            uri: Some(entry.uri.clone()),
            timestamp: entry.timestamp,
            line_number: entry.line_number,
        })
        .collect()
}

struct RequestRule {
    kind: &'static str,
    severity: &'static str,
    confidence: f32,
    needles: &'static [&'static str],
}

const REQUEST_RULES: [RequestRule; 4] = [
    RequestRule {
        kind: "sqlInjection",
        severity: "high",
        confidence: 0.9,
        needles: &[
            "union%20select",
            "union select",
            " or 1=1",
            "%27%20or%20",
            "information_schema",
            "sleep(",
            "benchmark(",
        ],
    },
    RequestRule {
        kind: "localFileInclusion",
        severity: "high",
        confidence: 0.9,
        needles: &["../", "..%2f", "/etc/passwd", "win.ini", "php://filter"],
    },
    RequestRule {
        kind: "crossSiteScripting",
        severity: "medium",
        confidence: 0.75,
        needles: &["<script", "%3cscript", "javascript:", "onerror=", "onload="],
    },
    RequestRule {
        kind: "scannerFingerprint",
        severity: "medium",
        confidence: 0.8,
        needles: &[
            "sqlmap",
            "nikto",
            "dirbuster",
            "gobuster",
            "wfuzz",
            "acunetix",
        ],
    },
];

fn detect_bruteforce_findings(entries: &[WebAccessLogEntry]) -> Vec<WebFinding> {
    let mut counts: HashMap<(&str, i64), u32> = HashMap::new();
    for entry in entries {
        if entry.method.eq_ignore_ascii_case("post") && looks_like_login_uri(&entry.uri) {
            if let Some(timestamp) = entry.timestamp {
                *counts
                    .entry((&entry.client_ip, timestamp.timestamp() / 300))
                    .or_default() += 1;
            }
        }
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count >= 50)
        .map(|((client_ip, bucket), count)| WebFinding {
            finding_kind: "bruteforce".to_string(),
            severity: "high".to_string(),
            confidence: 0.8,
            evidence: format!("{count} POST login requests in five-minute bucket {bucket}"),
            client_ip: Some(client_ip.to_string()),
            uri: None,
            timestamp: None,
            line_number: 0,
        })
        .collect()
}

fn looks_like_login_uri(uri: &str) -> bool {
    let lower = uri.to_ascii_lowercase();
    ["login", "signin", "wp-login", "admin"]
        .iter()
        .any(|needle| lower.contains(needle))
}
