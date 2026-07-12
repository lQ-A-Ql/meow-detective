use super::util::{dash_to_none, extract_quoted};
use super::{WebAccessLogEntry, WebErrorLogEntry};
use chrono::{DateTime, Utc};

pub fn parse_web_access_log(
    content: &str,
) -> Result<Vec<WebAccessLogEntry>, crate::LinuxArtifactError> {
    Ok(content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| parse_access_log_line(line, index as u64 + 1))
        .collect())
}

pub fn parse_web_error_log(
    content: &str,
) -> Result<Vec<WebErrorLogEntry>, crate::LinuxArtifactError> {
    Ok(content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            (!trimmed.is_empty()).then(|| WebErrorLogEntry {
                timestamp: extract_error_timestamp(trimmed),
                severity: extract_bracketed_severity(trimmed),
                message: trimmed.to_string(),
                line_number: index as u64 + 1,
            })
        })
        .collect())
}

fn parse_access_log_line(line: &str, line_number: u64) -> Option<WebAccessLogEntry> {
    let bracket_start = line.find('[')?;
    let bracket_end = line[bracket_start + 1..].find(']')? + bracket_start + 1;
    let client_ip = line[..bracket_start].split_whitespace().next()?.to_string();
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
