use super::super::shared::build_body_preview;
use chrono::{DateTime, Utc};
use mailparse::{parse_mail, DispositionType, MailAddr, MailHeaderMap, ParsedMail};
use std::collections::BTreeMap;
use transport::dto::{EmailAttachmentDto, EmailHeaderDto};

pub(crate) struct ParsedEmail {
    pub(crate) sent_at: Option<DateTime<Utc>>,
    pub(crate) received_at: Option<DateTime<Utc>>,
    pub(crate) from: String,
    pub(crate) to: Vec<String>,
    pub(crate) cc: Vec<String>,
    pub(crate) bcc: Vec<String>,
    pub(crate) reply_to: Option<String>,
    pub(crate) return_path: Option<String>,
    pub(crate) subject: String,
    pub(crate) message_id: String,
    pub(crate) in_reply_to: Option<String>,
    pub(crate) references: Vec<String>,
    pub(crate) attachments: Vec<String>,
    pub(crate) attachment_details: Vec<EmailAttachmentDto>,
    pub(crate) headers: Vec<EmailHeaderDto>,
    pub(crate) body_preview: String,
    pub(crate) body_plain: Option<String>,
    pub(crate) body_html: Option<String>,
    pub(crate) x_mailer: Option<String>,
    pub(crate) x_originating_ip: Option<String>,
    pub(crate) message_class: Option<String>,
}

pub(crate) fn parse_email_message(bytes: &[u8]) -> Result<ParsedEmail, String> {
    let stripped = strip_emlx_size_line(bytes);
    let raw = parse_mail(&stripped).map_err(|error| format!("mailparse error: {error}"))?;
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
        .map(|value| parse_message_ids(&value))
        .unwrap_or_default();
    let x_mailer = raw.headers.get_first_value("x-mailer");
    let x_originating_ip = raw.headers.get_first_value("x-originating-ip");
    let sent_at = raw
        .headers
        .get_first_value("date")
        .and_then(parse_email_datetime);
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
    if body_plain.is_none() && body_html.is_none() {
        if let Ok(body) = raw.get_body() {
            let trimmed = body.trim();
            if !trimmed.is_empty() {
                body_plain = Some(trimmed.to_string());
            }
        }
    }
    let attachments = attachment_details
        .iter()
        .map(|attachment| attachment.file_name.clone())
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
    let first_newline = bytes.iter().position(|byte| *byte == b'\n');
    let first_line_end = first_newline.map(|index| index + 1).unwrap_or(bytes.len());
    let first_line = &bytes[..first_line_end.saturating_sub(1)];
    if first_line.iter().all(|byte| byte.is_ascii_digit()) {
        bytes[first_line_end..].to_vec()
    } else {
        bytes.to_vec()
    }
}

fn collect_headers(mail: &ParsedMail) -> Vec<EmailHeaderDto> {
    mail.headers
        .iter()
        .map(|header| EmailHeaderDto {
            name: header.get_key().trim().to_string(),
            value: header.get_value().trim().to_string(),
        })
        .collect()
}

fn header_address_list(mail: &ParsedMail, name: &str) -> Vec<String> {
    let Some(header) = mail.headers.get_first_header(name) else {
        return Vec::new();
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
            .map(|id| {
                id.trim()
                    .trim_matches(|character| character == '<' || character == '>')
            })
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn format_address(address: MailAddr) -> String {
    match address {
        MailAddr::Single(single) => format_single_address(single),
        MailAddr::Group(group) => {
            let members = group
                .addrs
                .into_iter()
                .map(format_single_address)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}: {}", group.group_name, members)
        }
    }
}

fn format_single_address(single: mailparse::SingleInfo) -> String {
    match single.display_name {
        Some(name) if !name.trim().is_empty() => format!("{name} <{}>", single.addr),
        _ => single.addr,
    }
}

fn parse_email_datetime(value: String) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc2822(&value)
        .or_else(|_| DateTime::parse_from_rfc3339(&value))
        .ok()
        .map(|datetime| datetime.with_timezone(&Utc))
}

fn extract_received_datetime(mail: &ParsedMail) -> Option<DateTime<Utc>> {
    let received = mail
        .headers
        .iter()
        .filter(|header| header.get_key().eq_ignore_ascii_case("received"))
        .map(|header| header.get_value())
        .next_back()?;
    parse_email_datetime(received.rsplit(';').next()?.trim().to_string())
}

fn walk_parts(
    part: &ParsedMail,
    body_plain: &mut Option<String>,
    body_html: &mut Option<String>,
    attachments: &mut Vec<EmailAttachmentDto>,
) {
    let content_type = part.ctype.mimetype.to_lowercase();
    if content_type.starts_with("multipart/") {
        for subpart in &part.subparts {
            walk_parts(subpart, body_plain, body_html, attachments);
        }
        return;
    }
    let disposition = part.get_content_disposition();
    let is_attachment = matches!(disposition.disposition, DispositionType::Attachment)
        || (matches!(disposition.disposition, DispositionType::Inline)
            && !content_type.starts_with("text/")
            && disposition.params.contains_key("filename"));
    let has_filename = disposition.params.contains_key("filename");
    if is_attachment || has_filename {
        attachments.push(EmailAttachmentDto {
            file_name: attachment_name(&disposition.params, &part.ctype.params, &content_type),
            size: Some(part.get_body_raw().map_or(0, |body| body.len() as u64)),
            mime_type: Some(content_type),
            content_id: part.headers.get_first_value("content-id").map(|value| {
                value
                    .trim()
                    .trim_matches(|character| character == '<' || character == '>')
                    .to_string()
            }),
        });
    } else if content_type.starts_with("text/html") && body_html.is_none() {
        *body_html = part.get_body().ok();
    } else if content_type.starts_with("text/") && body_plain.is_none() {
        *body_plain = part.get_body().ok();
    }
}

fn attachment_name(
    content_disposition: &BTreeMap<String, String>,
    content_type: &BTreeMap<String, String>,
    mime_type: &str,
) -> String {
    for (key, value) in content_disposition {
        if key.eq_ignore_ascii_case("filename") || key.eq_ignore_ascii_case("filename*") {
            return decode_attachment_name(value);
        }
    }
    for (key, value) in content_type {
        if key.eq_ignore_ascii_case("name") || key.eq_ignore_ascii_case("name*") {
            return decode_attachment_name(value);
        }
    }
    if let Some(desc) = mime_type
        .split(';')
        .next()
        .and_then(|value| value.split('/').nth(1))
    {
        if !desc.trim().is_empty() {
            return format!("unnamed.{}", desc.trim());
        }
    }
    "unnamed".to_string()
}

fn decode_attachment_name(name: &str) -> String {
    let name = name.trim().trim_matches('"');
    for prefix in ["UTF-8'", "utf-8'"] {
        if let Some(stripped) = name.strip_prefix(prefix) {
            return stripped.find('\'').map_or_else(
                || percent_decode(stripped),
                |index| percent_decode(&stripped[index + 1..]),
            );
        }
    }
    name.to_string()
}

fn percent_decode(input: &str) -> String {
    let mut output = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or(""),
                16,
            ) {
                output.push(byte);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}
