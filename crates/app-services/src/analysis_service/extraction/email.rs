use super::ExtractionOutcome;
use crate::analysis_service::artifact_builders::{
    base_attrs, make_artifact, make_timeline_event, string_array_value, title_or_url,
};
use crate::analysis_service::candidates::EvidenceCandidate;
use chrono::{DateTime, Utc};
use serde_json::Value;

pub(super) fn extract_email_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let parsed = parse_email_message(bytes);
    let mut attrs = base_attrs(candidate);
    attrs.insert("from".to_string(), Value::String(parsed.from.clone()));
    attrs.insert("to".to_string(), string_array_value(&parsed.to));
    attrs.insert("cc".to_string(), string_array_value(&parsed.cc));
    attrs.insert("bcc".to_string(), string_array_value(&parsed.bcc));
    attrs.insert("subject".to_string(), Value::String(parsed.subject.clone()));
    attrs.insert(
        "messageId".to_string(),
        Value::String(parsed.message_id.clone()),
    );
    attrs.insert(
        "attachments".to_string(),
        string_array_value(&parsed.attachments),
    );
    attrs.insert(
        "bodyPreview".to_string(),
        Value::String(parsed.body_preview.clone()),
    );
    if let Some(sent_at) = parsed.sent_at {
        attrs.insert("sentAt".to_string(), Value::String(sent_at.to_rfc3339()));
    }
    let mut outcome = ExtractionOutcome::default();
    if let Some(sent_at) = parsed.sent_at {
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            "EMAIL_SENT",
            sent_at,
            format!("Email: {}", title_or_url(&parsed.subject, &candidate.path)),
            parsed.from.clone(),
            attrs.clone(),
            "email.eml_emlx",
        ));
    }
    outcome.artifacts.push(make_artifact(
        "EmailMessage",
        format!("Email: {}", title_or_url(&parsed.subject, &candidate.path)),
        parsed.from,
        candidate,
        "email.eml_emlx",
        attrs,
    ));
    outcome
}

struct ParsedEmail {
    sent_at: Option<DateTime<Utc>>,
    from: String,
    to: Vec<String>,
    cc: Vec<String>,
    bcc: Vec<String>,
    subject: String,
    message_id: String,
    attachments: Vec<String>,
    body_preview: String,
}

fn parse_email_message(bytes: &[u8]) -> ParsedEmail {
    let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    let text = strip_emlx_size_line(&text);
    let (header_text, body_text) = text.split_once("\n\n").unwrap_or((text.as_str(), ""));
    let headers = parse_headers(header_text);
    let date = header_value(&headers, "date");
    ParsedEmail {
        sent_at: date.and_then(parse_email_datetime),
        from: header_value(&headers, "from").unwrap_or_default(),
        to: split_address_list(header_value(&headers, "to").unwrap_or_default()),
        cc: split_address_list(header_value(&headers, "cc").unwrap_or_default()),
        bcc: split_address_list(header_value(&headers, "bcc").unwrap_or_default()),
        subject: header_value(&headers, "subject").unwrap_or_default(),
        message_id: header_value(&headers, "message-id").unwrap_or_default(),
        attachments: extract_attachment_names(&text),
        body_preview: body_text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take(8)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(500)
            .collect(),
    }
}

fn strip_emlx_size_line(text: &str) -> String {
    let Some((first, rest)) = text.split_once('\n') else {
        return text.to_string();
    };
    if first.chars().all(|ch| ch.is_ascii_digit()) {
        rest.to_string()
    } else {
        text.to_string()
    }
}

fn parse_headers(header_text: &str) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in header_text.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some((_, value)) = headers.last_mut() {
                value.push(' ');
                value.push_str(line.trim());
            }
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    headers
}

fn header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(header_name, _)| header_name == name)
        .map(|(_, value)| value.clone())
}

fn split_address_list(value: String) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_email_datetime(value: String) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc2822(&value)
        .or_else(|_| DateTime::parse_from_rfc3339(&value))
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn extract_attachment_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for marker in ["filename=", "name="] {
        let mut rest = text;
        while let Some((_, tail)) = rest.split_once(marker) {
            let trimmed = tail.trim_start_matches([' ', '\t']);
            let (name, next) = if let Some(stripped) = trimmed.strip_prefix('"') {
                stripped.split_once('"').unwrap_or((stripped, ""))
            } else {
                let end = trimmed
                    .find(|ch: char| ch == ';' || ch == '\n' || ch.is_whitespace())
                    .unwrap_or(trimmed.len());
                (&trimmed[..end], &trimmed[end..])
            };
            if !name.trim().is_empty() && !names.iter().any(|existing| existing == name) {
                names.push(name.trim().to_string());
            }
            rest = next;
        }
    }
    names
}
