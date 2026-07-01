//! Single-message EML/EMLX parsing and extraction.

use super::super::ExtractionOutcome;
use super::shared::build_body_preview;
use crate::analysis_service::artifact_builders::{
    base_attrs, make_artifact, make_timeline_event, string_array_value, title_or_url,
};
use crate::analysis_service::candidates::EvidenceCandidate;
use chrono::{DateTime, Utc};
use mailparse::{parse_mail, DispositionType, MailAddr, MailHeaderMap, ParsedMail};
use serde_json::Value;
use transport::dto::{EmailAttachmentDto, EmailHeaderDto};

const EMAIL_EXTRACTOR_ID: &str = "email.eml_emlx";

pub(super) fn extract_eml_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    let parsed = match parse_email_message(bytes) {
        Ok(p) => p,
        Err(err) => {
            outcome
                .warnings
                .push(format!("EML parse error for {}: {}", candidate.path, err));
            return outcome;
        }
    };
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
        "attachmentDetails".to_string(),
        Value::Array(
            parsed
                .attachment_details
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "fileName": a.file_name,
                        "size": a.size,
                        "mimeType": a.mime_type,
                        "contentId": a.content_id,
                    })
                })
                .collect(),
        ),
    );
    attrs.insert(
        "headers".to_string(),
        Value::Array(
            parsed
                .headers
                .iter()
                .map(|h| serde_json::json!({"name": h.name, "value": h.value}))
                .collect(),
        ),
    );
    attrs.insert(
        "bodyPreview".to_string(),
        Value::String(parsed.body_preview.clone()),
    );
    if let Some(body_plain) = &parsed.body_plain {
        attrs.insert("bodyPlain".to_string(), Value::String(body_plain.clone()));
    }
    if let Some(body_html) = &parsed.body_html {
        attrs.insert("bodyHtml".to_string(), Value::String(body_html.clone()));
    }
    if let Some(sent_at) = parsed.sent_at {
        attrs.insert("sentAt".to_string(), Value::String(sent_at.to_rfc3339()));
    }
    if let Some(received_at) = parsed.received_at {
        attrs.insert(
            "receivedAt".to_string(),
            Value::String(received_at.to_rfc3339()),
        );
    }
    if let Some(reply_to) = &parsed.reply_to {
        attrs.insert("replyTo".to_string(), Value::String(reply_to.clone()));
    }
    if let Some(return_path) = &parsed.return_path {
        attrs.insert("returnPath".to_string(), Value::String(return_path.clone()));
    }
    if let Some(in_reply_to) = &parsed.in_reply_to {
        attrs.insert("inReplyTo".to_string(), Value::String(in_reply_to.clone()));
    }
    if !parsed.references.is_empty() {
        attrs.insert(
            "references".to_string(),
            string_array_value(&parsed.references),
        );
    }
    if let Some(x_mailer) = &parsed.x_mailer {
        attrs.insert("xMailer".to_string(), Value::String(x_mailer.clone()));
    }
    if let Some(x_originating_ip) = &parsed.x_originating_ip {
        attrs.insert(
            "xOriginatingIp".to_string(),
            Value::String(x_originating_ip.clone()),
        );
    }
    if let Some(message_class) = &parsed.message_class {
        attrs.insert(
            "messageClass".to_string(),
            Value::String(message_class.clone()),
        );
    }
    attrs.insert(
        "attachmentCount".to_string(),
        Value::Number(serde_json::Number::from(parsed.attachments.len())),
    );
    // Single EML files carry no deleted-item metadata.
    attrs.insert("isDeleted".to_string(), Value::Null);

    let event_time = parsed.sent_at.or(parsed.received_at);
    if let Some(event_time) = event_time {
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            "EMAIL_SENT",
            event_time,
            format!("Email: {}", title_or_url(&parsed.subject, &candidate.path)),
            parsed.from.clone(),
            attrs.clone(),
            EMAIL_EXTRACTOR_ID,
        ));
    }
    outcome.artifacts.push(make_artifact(
        "EmailMessage",
        format!("Email: {}", title_or_url(&parsed.subject, &candidate.path)),
        parsed.from,
        candidate,
        EMAIL_EXTRACTOR_ID,
        attrs,
    ));
    outcome
}

pub(super) struct ParsedEmail {
    pub(super) sent_at: Option<DateTime<Utc>>,
    pub(super) received_at: Option<DateTime<Utc>>,
    pub(super) from: String,
    pub(super) to: Vec<String>,
    pub(super) cc: Vec<String>,
    pub(super) bcc: Vec<String>,
    pub(super) reply_to: Option<String>,
    pub(super) return_path: Option<String>,
    pub(super) subject: String,
    pub(super) message_id: String,
    pub(super) in_reply_to: Option<String>,
    pub(super) references: Vec<String>,
    pub(super) attachments: Vec<String>,
    pub(super) attachment_details: Vec<EmailAttachmentDto>,
    pub(super) headers: Vec<EmailHeaderDto>,
    pub(super) body_preview: String,
    pub(super) body_plain: Option<String>,
    pub(super) body_html: Option<String>,
    pub(super) x_mailer: Option<String>,
    pub(super) x_originating_ip: Option<String>,
    pub(super) message_class: Option<String>,
}

pub(super) fn parse_email_message(bytes: &[u8]) -> Result<ParsedEmail, String> {
    let stripped = strip_emlx_size_line(bytes);
    let raw = match parse_mail(&stripped) {
        Ok(m) => m,
        Err(err) => return Err(format!("mailparse error: {}", err)),
    };

    let headers = collect_headers(&raw);
    let from = header_address_list(&raw, "from")
        .first()
        .cloned()
        .unwrap_or_default();
    let to = header_address_list(&raw, "to");
    let cc = header_address_list(&raw, "cc");
    let bcc = header_address_list(&raw, "bcc");
    let reply_to = raw.headers.get_first_value("reply-to");
    let return_path = raw.headers.get_first_value("return-path");
    let subject = raw.headers.get_first_value("subject").unwrap_or_default();
    let message_id = raw
        .headers
        .get_first_value("message-id")
        .unwrap_or_default();
    let in_reply_to = raw.headers.get_first_value("in-reply-to");
    let references = raw
        .headers
        .get_first_value("references")
        .map(|v| parse_message_ids(&v))
        .unwrap_or_default();
    let x_mailer = raw.headers.get_first_value("x-mailer");
    let x_originating_ip = raw.headers.get_first_value("x-originating-ip");
    let date = raw.headers.get_first_value("date");
    let sent_at = date.and_then(parse_email_datetime);
    let received_at = extract_received_datetime(&raw);
    let message_class = raw.headers.get_first_value("x-message-class");

    let mut body_plain = None;
    let mut body_html = None;
    let mut attachment_details = Vec::new();
    walk_parts(
        &raw,
        &mut body_plain,
        &mut body_html,
        &mut attachment_details,
    );

    // Fallback: malformed single-part messages may declare the entire message
    // as an attachment via a top-level Content-Disposition header. In that case
    // the body is still meaningful, so treat it as plain text when no text part
    // has been extracted yet.
    if body_plain.is_none() && body_html.is_none() {
        if let Ok(body) = raw.get_body() {
            let trimmed = body.trim();
            if !trimmed.is_empty() {
                body_plain = Some(trimmed.to_string());
            }
        }
    }

    let attachments: Vec<String> = attachment_details
        .iter()
        .map(|a| a.file_name.clone())
        .collect();
    let preview_source = body_plain.clone().or_else(|| body_html.clone());
    let body_preview = preview_source
        .as_ref()
        .map(|text| build_body_preview(text))
        .unwrap_or_default();

    Ok(ParsedEmail {
        sent_at,
        received_at,
        from,
        to,
        cc,
        bcc,
        reply_to,
        return_path,
        subject,
        message_id,
        in_reply_to,
        references,
        attachments,
        attachment_details,
        headers,
        body_preview,
        body_plain,
        body_html,
        x_mailer,
        x_originating_ip,
        message_class,
    })
}

fn strip_emlx_size_line(bytes: &[u8]) -> Vec<u8> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let first_newline = bytes.iter().position(|b| *b == b'\n');
    let first_line_end = first_newline.map(|i| i + 1).unwrap_or(bytes.len());
    let first_line = &bytes[..first_line_end.saturating_sub(1)];
    let is_size_line = first_line.iter().all(|b| b.is_ascii_digit());
    if is_size_line {
        bytes[first_line_end..].to_vec()
    } else {
        bytes.to_vec()
    }
}

fn collect_headers(mail: &ParsedMail) -> Vec<EmailHeaderDto> {
    mail.headers
        .iter()
        .map(|h| EmailHeaderDto {
            name: h.get_key().trim().to_string(),
            value: h.get_value().trim().to_string(),
        })
        .collect()
}

fn header_address_list(mail: &ParsedMail, name: &str) -> Vec<String> {
    let header = match mail.headers.get_first_header(name) {
        Some(h) => h,
        None => return Vec::new(),
    };
    match mailparse::addrparse_header(header) {
        Ok(list) => list.into_inner().into_iter().map(format_address).collect(),
        Err(_) => Vec::new(),
    }
}

fn parse_message_ids(raw: &str) -> Vec<String> {
    match mailparse::msgidparse(raw) {
        Ok(list) => list
            .iter()
            .map(|id| id.trim().trim_matches(|c| c == '<' || c == '>').to_string())
            .filter(|id| !id.is_empty())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn format_address(addr: MailAddr) -> String {
    match addr {
        MailAddr::Single(s) => {
            if let Some(name) = s.display_name {
                if name.trim().is_empty() {
                    s.addr
                } else {
                    format!("{} <{}>", name, s.addr)
                }
            } else {
                s.addr
            }
        }
        MailAddr::Group(g) => {
            let members = g
                .addrs
                .into_iter()
                .map(|s| {
                    if let Some(name) = s.display_name {
                        if name.trim().is_empty() {
                            s.addr
                        } else {
                            format!("{} <{}>", name, s.addr)
                        }
                    } else {
                        s.addr
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}: {}", g.group_name, members)
        }
    }
}

fn parse_email_datetime(value: String) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc2822(&value)
        .or_else(|_| DateTime::parse_from_rfc3339(&value))
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Extract the delivery timestamp from the last `Received` header.
///
/// Received headers typically end with a semicolon followed by a date, e.g.:
/// `...; Mon, 15 Jan 2024 09:30:00 +0000`. We take the last such date.
fn extract_received_datetime(mail: &ParsedMail) -> Option<DateTime<Utc>> {
    let received = mail
        .headers
        .iter()
        .filter(|h| h.get_key().eq_ignore_ascii_case("received"))
        .map(|h| h.get_value())
        .next_back()?;
    let date_part = received.rsplit(';').next()?.trim();
    parse_email_datetime(date_part.to_string())
}

fn walk_parts(
    part: &ParsedMail,
    body_plain: &mut Option<String>,
    body_html: &mut Option<String>,
    attachments: &mut Vec<EmailAttachmentDto>,
) {
    let ct_lower = part.ctype.mimetype.to_lowercase();

    if ct_lower.starts_with("multipart/") {
        for sub in &part.subparts {
            walk_parts(sub, body_plain, body_html, attachments);
        }
        return;
    }

    let cd = part.get_content_disposition();
    let is_attachment = matches!(cd.disposition, DispositionType::Attachment)
        || (matches!(cd.disposition, DispositionType::Inline)
            && !ct_lower.starts_with("text/")
            && cd.params.contains_key("filename"));
    let has_filename = cd.params.contains_key("filename");

    if is_attachment || has_filename {
        let name = attachment_name(&cd.params, &part.ctype.params, &ct_lower);
        let size = part.get_body_raw().map(|b| b.len() as u64).unwrap_or(0);
        let content_id = part
            .headers
            .get_first_value("content-id")
            .map(|v| v.trim().trim_matches(|c| c == '<' || c == '>').to_string());
        attachments.push(EmailAttachmentDto {
            file_name: name,
            size: Some(size),
            mime_type: Some(ct_lower),
            content_id,
        });
    } else if ct_lower.starts_with("text/html") && body_html.is_none() {
        *body_html = part.get_body().ok();
    } else if ct_lower.starts_with("text/") && body_plain.is_none() {
        *body_plain = part.get_body().ok();
    }
}

fn attachment_name(
    cd_params: &std::collections::BTreeMap<String, String>,
    ct_params: &std::collections::BTreeMap<String, String>,
    ct_lower: &str,
) -> String {
    for (key, value) in cd_params {
        if key.eq_ignore_ascii_case("filename") || key.eq_ignore_ascii_case("filename*") {
            return decode_attachment_name(value);
        }
    }
    for (key, value) in ct_params {
        if key.eq_ignore_ascii_case("name") || key.eq_ignore_ascii_case("name*") {
            return decode_attachment_name(value);
        }
    }
    if let Some(desc) = ct_lower.split(';').next().and_then(|s| s.split('/').nth(1)) {
        if !desc.trim().is_empty() {
            return format!("unnamed.{}", desc.trim());
        }
    }
    "unnamed".to_string()
}

fn decode_attachment_name(name: &str) -> String {
    let name = name.trim().trim_matches('"');
    if let Some(stripped) = name.strip_prefix("UTF-8'") {
        if let Some(idx) = stripped.find('\'') {
            return percent_decode(&stripped[idx + 1..]);
        }
        return percent_decode(stripped);
    }
    if let Some(stripped) = name.strip_prefix("utf-8'") {
        if let Some(idx) = stripped.find('\'') {
            return percent_decode(&stripped[idx + 1..]);
        }
        return percent_decode(stripped);
    }
    name.to_string()
}

fn percent_decode(input: &str) -> String {
    let mut output = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                output.push(byte);
                i += 3;
                continue;
            }
        }
        output.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}
