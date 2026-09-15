use super::util::{dash_to_none, extract_quoted};
use super::{WebAccessLogEntry, WebErrorLogEntry};
use crate::stats::{LogLineStats, WebAccessLogStats};
use chrono::{DateTime, Utc};
use std::net::IpAddr;

pub fn parse_web_access_log(
    content: &str,
) -> Result<Vec<WebAccessLogEntry>, crate::LinuxArtifactError> {
    let (entries, _) = parse_web_access_log_with_stats(content)?;
    Ok(entries)
}

pub fn parse_web_access_log_with_stats(
    content: &str,
) -> Result<(Vec<WebAccessLogEntry>, WebAccessLogStats), crate::LinuxArtifactError> {
    let mut entries = Vec::new();
    let mut stats = WebAccessLogStats::default();
    for (index, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        stats.total_lines += 1;
        if let Some(entry) = parse_access_log_line(line, index as u64 + 1) {
            stats.parsed_lines += 1;
            if entry.vhost.is_some() {
                stats.vhost_prefixed_lines += 1;
            }
            entries.push(entry);
        }
    }
    Ok((entries, stats))
}

pub fn parse_web_error_log(
    content: &str,
) -> Result<Vec<WebErrorLogEntry>, crate::LinuxArtifactError> {
    let (entries, _) = parse_web_error_log_with_stats(content)?;
    Ok(entries)
}

pub fn parse_web_error_log_with_stats(
    content: &str,
) -> Result<(Vec<WebErrorLogEntry>, LogLineStats), crate::LinuxArtifactError> {
    let mut entries = Vec::new();
    let mut stats = LogLineStats::default();
    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        stats.total_lines += 1;
        stats.parsed_lines += 1;
        entries.push(WebErrorLogEntry {
            timestamp: extract_error_timestamp(trimmed),
            severity: extract_bracketed_severity(trimmed),
            message: trimmed.to_string(),
            line_number: index as u64 + 1,
        });
    }
    Ok((entries, stats))
}

fn parse_access_log_line(line: &str, line_number: u64) -> Option<WebAccessLogEntry> {
    let bracket_start = line.find('[')?;
    let bracket_end = line[bracket_start + 1..].find(']')? + bracket_start + 1;
    let (client_ip, vhost) = client_and_vhost(&line[..bracket_start])?;
    let timestamp = DateTime::parse_from_str(
        &line[bracket_start + 1..bracket_end],
        "%d/%b/%Y:%H:%M:%S %z",
    )
    .ok()
    .map(|dt| dt.with_timezone(&Utc));

    let mut rest = line[bracket_end + 1..].trim();
    let (request, next) = extract_quoted(rest)?;
    rest = next.trim();
    let mut status_parts = rest.splitn(3, ' ');
    let status = status_parts.next()?.parse::<u16>().ok()?;
    let response_bytes = status_parts.next().and_then(|raw| match raw {
        "-" => None,
        value => value.parse::<u64>().ok(),
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
        vhost,
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

/// Split the pre-timestamp fields into (client address, virtual host).
/// Apache `%v` and nginx `$host`-prefixed formats lead the line with the
/// virtual-host name; when the first token is not an IP address but the
/// second is, the second token is the client and the first is annotated as
/// the vhost so the host name is never silently reported as `client_ip`.
/// A non-IP first token followed by `-`/`%u` (HostnameLookups clients) keeps
/// the first token as the client, matching the historic behavior.
fn client_and_vhost(prefix: &str) -> Option<(String, Option<String>)> {
    let mut tokens = prefix.split_whitespace();
    let first = tokens.next()?;
    if first.parse::<IpAddr>().is_err() {
        if let Some(second) = tokens.next() {
            if second.parse::<IpAddr>().is_ok() {
                return Some((second.to_string(), Some(first.to_string())));
            }
        }
    }
    Some((first.to_string(), None))
}

fn extract_bracketed_severity(line: &str) -> Option<String> {
    let start = line.find('[')?;
    let end = line[start + 1..].find(']')? + start + 1;
    let value = &line[start + 1..end];
    (value.contains("error") || value.contains("warn") || value.contains("notice"))
        .then(|| value.to_string())
}

fn extract_error_timestamp(line: &str) -> Option<String> {
    if line.starts_with('[') {
        return line
            .find(']')
            .map(|end| line[..=end].trim_matches(['[', ']']).to_string());
    }
    (line.len() >= 19 && line.as_bytes().get(4) == Some(&b'/')).then(|| line[..19].to_string())
}
