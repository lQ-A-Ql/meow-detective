use super::ExtractionOutcome;
use crate::analysis_service::artifact_builders::{
    base_attrs, make_artifact, make_timeline_event, string_array_value, title_or_url,
};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::MAX_ANALYSIS_SOURCE_BYTES;
use chrono::{DateTime, Utc};
use mailparse::{parse_mail, DispositionType, MailAddr, MailHeaderMap, ParsedMail};
use serde_json::Value;
use transport::dto::{EmailAttachmentDto, EmailHeaderDto};

const EMAIL_EXTRACTOR_ID: &str = "email.eml_emlx";
const MBOX_EXTRACTOR_ID: &str = "email.mbox";
const BODY_PREVIEW_MAX_LEN: usize = 500;
const BODY_PREVIEW_MAX_LINES: usize = 8;

pub(super) fn extract_email_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let path_lower = candidate.path.to_lowercase();
    if path_lower.ends_with(".mbox") {
        extract_mbox_candidate(candidate, bytes)
    } else if path_lower.ends_with(".pst") || path_lower.ends_with(".ost") {
        extract_pst_candidate(candidate, bytes)
    } else {
        extract_eml_candidate(candidate, bytes)
    }
}

fn extract_eml_candidate(candidate: &EvidenceCandidate, bytes: &[u8]) -> ExtractionOutcome {
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

fn extract_mbox_candidate(candidate: &EvidenceCandidate, bytes: &[u8]) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    let container_path = std::path::Path::new(&candidate.path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&candidate.path)
        .to_string();

    let messages = match containers_pst::mbox::parse_mbox(bytes) {
        Ok(msgs) => msgs,
        Err(err) => {
            outcome
                .warnings
                .push(format!("mbox parse error for {}: {}", candidate.path, err));
            return outcome;
        }
    };

    for msg in messages {
        let from = if msg.sender_email.is_empty() {
            msg.sender_name.clone()
        } else if msg.sender_name.is_empty() {
            msg.sender_email.clone()
        } else {
            format!("{} <{}>", msg.sender_name, msg.sender_email)
        };
        let to = msg.to.clone();
        let cc = msg.cc.clone();
        let bcc = msg.bcc.clone();
        let subject = msg.subject.clone();
        let sent_at = msg.sent_time;
        let received_at = msg.received_time;
        let body_preview = build_body_preview(&msg.body_plain);
        let attachments: Vec<String> = msg
            .attachments
            .iter()
            .map(|a| a.name.clone())
            .filter(|n| !n.is_empty())
            .collect();
        let attachment_details: Vec<EmailAttachmentDto> = msg
            .attachments
            .iter()
            .map(|a| EmailAttachmentDto {
                file_name: a.name.clone(),
                size: Some(a.size),
                mime_type: Some(a.mime_type.clone()),
                content_id: a.content_id.clone(),
            })
            .collect();

        let mut attrs = base_attrs(candidate);
        attrs.insert("from".to_string(), Value::String(from.clone()));
        attrs.insert("to".to_string(), string_array_value(&to));
        attrs.insert("cc".to_string(), string_array_value(&cc));
        attrs.insert("bcc".to_string(), string_array_value(&bcc));
        attrs.insert("subject".to_string(), Value::String(subject.clone()));
        attrs.insert(
            "messageId".to_string(),
            Value::String(msg.message_id.clone()),
        );
        attrs.insert("attachments".to_string(), string_array_value(&attachments));
        attrs.insert(
            "attachmentDetails".to_string(),
            Value::Array(
                attachment_details
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
                msg.headers
                    .iter()
                    .map(|(name, value)| serde_json::json!({"name": name, "value": value}))
                    .collect(),
            ),
        );
        attrs.insert(
            "bodyPreview".to_string(),
            Value::String(body_preview.clone()),
        );
        if !msg.body_plain.is_empty() {
            attrs.insert(
                "bodyPlain".to_string(),
                Value::String(msg.body_plain.clone()),
            );
        }
        if !msg.body_html.is_empty() {
            attrs.insert("bodyHtml".to_string(), Value::String(msg.body_html.clone()));
        }
        if let Some(sent_at) = sent_at {
            attrs.insert("sentAt".to_string(), Value::String(sent_at.to_rfc3339()));
        }
        if let Some(received_at) = received_at {
            attrs.insert(
                "receivedAt".to_string(),
                Value::String(received_at.to_rfc3339()),
            );
        }
        if !msg.reply_to.is_empty() {
            attrs.insert("replyTo".to_string(), Value::String(msg.reply_to.clone()));
        }
        if !msg.return_path.is_empty() {
            attrs.insert(
                "returnPath".to_string(),
                Value::String(msg.return_path.clone()),
            );
        }
        if !msg.in_reply_to.is_empty() {
            attrs.insert(
                "inReplyTo".to_string(),
                Value::String(msg.in_reply_to.clone()),
            );
        }
        if !msg.references.is_empty() {
            attrs.insert(
                "references".to_string(),
                string_array_value(&msg.references),
            );
        }
        if !msg.message_class.is_empty() {
            attrs.insert(
                "messageClass".to_string(),
                Value::String(msg.message_class.clone()),
            );
        }
        if !msg.x_mailer.is_empty() {
            attrs.insert("xMailer".to_string(), Value::String(msg.x_mailer.clone()));
        }
        if !msg.x_originating_ip.is_empty() {
            attrs.insert(
                "xOriginatingIp".to_string(),
                Value::String(msg.x_originating_ip.clone()),
            );
        }
        attrs.insert(
            "containerPath".to_string(),
            Value::String(container_path.clone()),
        );
        attrs.insert(
            "attachmentCount".to_string(),
            Value::Number(serde_json::Number::from(attachments.len())),
        );
        attrs.insert("isDeleted".to_string(), Value::Null);

        let event_time = sent_at.or(received_at);
        if let Some(event_time) = event_time {
            outcome.timeline_events.push(make_timeline_event(
                &candidate.file_id,
                "EMAIL_SENT",
                event_time,
                format!("Email: {}", title_or_url(&subject, &candidate.path)),
                from.clone(),
                attrs.clone(),
                MBOX_EXTRACTOR_ID,
            ));
        }
        outcome.artifacts.push(make_artifact(
            "EmailMessage",
            format!("Email: {}", title_or_url(&subject, &candidate.path)),
            from,
            candidate,
            MBOX_EXTRACTOR_ID,
            attrs,
        ));
    }

    outcome
}

const PST_EXTRACTOR_ID: &str = "email.pst";
const OST_EXTRACTOR_ID: &str = "email.ost";

fn extract_pst_candidate(candidate: &EvidenceCandidate, bytes: &[u8]) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();

    if bytes.len() < 4 || &bytes[0..4] != b"!BDN" {
        outcome.warnings.push(format!(
            "PST/OST file {} does not have expected magic bytes",
            candidate.path
        ));
        return outcome;
    }

    // PST/OST readers need the complete file. Skip files that exceed the
    // analysis byte budget to avoid loading multi-GB archives into memory.
    if candidate.size > MAX_ANALYSIS_SOURCE_BYTES as u64 {
        outcome.warnings.push(format!(
            "PST/OST file {} ({} bytes) exceeds analysis source byte limit ({}); skipped",
            candidate.path, candidate.size, MAX_ANALYSIS_SOURCE_BYTES
        ));
        return outcome;
    }

    // Best-effort encryption detection from the PST header.
    if is_pst_encrypted(bytes) {
        outcome.warnings.push(format!(
            "PST/OST file {} appears to be encrypted; skipping",
            candidate.path
        ));
        return outcome;
    }

    let mut temp_file = match tempfile::NamedTempFile::with_suffix(".pst") {
        Ok(f) => f,
        Err(err) => {
            outcome.warnings.push(format!(
                "failed to create temp file for {}: {}",
                candidate.path, err
            ));
            return outcome;
        }
    };

    if let Err(err) = std::io::Write::write_all(&mut temp_file, bytes) {
        outcome.warnings.push(format!(
            "failed to write temp file for {}: {}",
            candidate.path, err
        ));
        return outcome;
    }

    let path = temp_file.path().to_path_buf();
    let path_lower = candidate.path.to_lowercase();
    let extractor_id = if path_lower.ends_with(".ost") {
        OST_EXTRACTOR_ID
    } else {
        PST_EXTRACTOR_ID
    };

    let messages: Vec<containers_pst::PstMessage> = if path_lower.ends_with(".ost") {
        match containers_pst::ost::OstReader::open(&path) {
            Ok(reader) => match reader.read_messages() {
                Ok(msgs) => msgs,
                Err(err) => {
                    outcome.warnings.push(format!(
                        "OST read_messages error for {}: {}",
                        candidate.path, err
                    ));
                    Vec::new()
                }
            },
            Err(err) => {
                outcome
                    .warnings
                    .push(format!("OST open error for {}: {}", candidate.path, err));
                Vec::new()
            }
        }
    } else {
        match containers_pst::pst::PstReader::open(&path) {
            Ok(reader) => match reader.read_messages() {
                Ok(msgs) => msgs,
                Err(err) => {
                    outcome.warnings.push(format!(
                        "PST read_messages error for {}: {}",
                        candidate.path, err
                    ));
                    Vec::new()
                }
            },
            Err(err) => {
                outcome
                    .warnings
                    .push(format!("PST open error for {}: {}", candidate.path, err));
                Vec::new()
            }
        }
    };

    let container_path = std::path::Path::new(&candidate.path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&candidate.path)
        .to_string();

    for msg in messages {
        let from = if msg.sender_email.is_empty() {
            msg.sender_name.clone()
        } else if msg.sender_name.is_empty() {
            msg.sender_email.clone()
        } else {
            format!("{} <{}>", msg.sender_name, msg.sender_email)
        };
        let to = msg.to.clone();
        let cc = msg.cc.clone();
        let bcc = msg.bcc.clone();
        let subject = msg.subject.clone();
        let sent_at = msg.sent_time;
        let received_at = msg.received_time;
        let body_preview = build_body_preview(&msg.body_plain);
        let attachments: Vec<String> = msg
            .attachments
            .iter()
            .map(|a| a.name.clone())
            .filter(|n| !n.is_empty())
            .collect();
        let attachment_details: Vec<EmailAttachmentDto> = msg
            .attachments
            .iter()
            .map(|a| EmailAttachmentDto {
                file_name: a.name.clone(),
                size: Some(a.size),
                mime_type: Some(a.mime_type.clone()),
                content_id: a.content_id.clone(),
            })
            .collect();
        let folder_path = if msg.folder_path.is_empty() {
            container_path.clone()
        } else {
            format!("{}/{}", container_path, msg.folder_path)
        };

        let mut attrs = base_attrs(candidate);
        attrs.insert("from".to_string(), Value::String(from.clone()));
        attrs.insert("to".to_string(), string_array_value(&to));
        attrs.insert("cc".to_string(), string_array_value(&cc));
        attrs.insert("bcc".to_string(), string_array_value(&bcc));
        attrs.insert("subject".to_string(), Value::String(subject.clone()));
        attrs.insert(
            "messageId".to_string(),
            Value::String(msg.message_id.clone()),
        );
        attrs.insert("attachments".to_string(), string_array_value(&attachments));
        attrs.insert(
            "attachmentDetails".to_string(),
            Value::Array(
                attachment_details
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
                msg.headers
                    .iter()
                    .map(|(name, value)| serde_json::json!({"name": name, "value": value}))
                    .collect(),
            ),
        );
        attrs.insert(
            "bodyPreview".to_string(),
            Value::String(body_preview.clone()),
        );
        if !msg.body_plain.is_empty() {
            attrs.insert(
                "bodyPlain".to_string(),
                Value::String(msg.body_plain.clone()),
            );
        }
        if !msg.body_html.is_empty() {
            attrs.insert("bodyHtml".to_string(), Value::String(msg.body_html.clone()));
        }
        if let Some(sent_at) = sent_at {
            attrs.insert("sentAt".to_string(), Value::String(sent_at.to_rfc3339()));
        }
        if let Some(received_at) = received_at {
            attrs.insert(
                "receivedAt".to_string(),
                Value::String(received_at.to_rfc3339()),
            );
        }
        if !msg.in_reply_to.is_empty() {
            attrs.insert(
                "inReplyTo".to_string(),
                Value::String(msg.in_reply_to.clone()),
            );
        }
        if !msg.references.is_empty() {
            attrs.insert(
                "references".to_string(),
                string_array_value(&msg.references),
            );
        }
        attrs.insert(
            "containerPath".to_string(),
            Value::String(folder_path.clone()),
        );
        if !msg.message_class.is_empty() {
            attrs.insert(
                "messageClass".to_string(),
                Value::String(msg.message_class.clone()),
            );
        }
        attrs.insert(
            "attachmentCount".to_string(),
            Value::Number(serde_json::Number::from(attachments.len())),
        );
        let is_deleted = is_deleted_folder_path(&folder_path);
        attrs.insert("isDeleted".to_string(), Value::Bool(is_deleted));

        let event_time = sent_at.or(received_at);
        if let Some(event_time) = event_time {
            outcome.timeline_events.push(make_timeline_event(
                &candidate.file_id,
                "EMAIL_SENT",
                event_time,
                format!("Email: {}", title_or_url(&subject, &candidate.path)),
                from.clone(),
                attrs.clone(),
                extractor_id,
            ));
        }
        outcome.artifacts.push(make_artifact(
            "EmailMessage",
            format!("Email: {}", title_or_url(&subject, &candidate.path)),
            from,
            candidate,
            extractor_id,
            attrs,
        ));
    }

    outcome
}

fn is_pst_encrypted(bytes: &[u8]) -> bool {
    // PST header offset 0x1CC (bCryptMethod) indicates encryption:
    // 0 = no encryption, 1 = compressible encryption, 2 = high encryption.
    const CRYPT_METHOD_OFFSET: usize = 0x1CC;
    if bytes.len() <= CRYPT_METHOD_OFFSET {
        return false;
    }
    let method = bytes[CRYPT_METHOD_OFFSET];
    method != 0
}

fn is_deleted_folder_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    const DELETED_FOLDERS: &[&str] = &[
        "deleted items",
        "已删除邮件",
        "trash",
        "junk",
        "spam",
        "deleted",
    ];
    DELETED_FOLDERS
        .iter()
        .any(|name| lower.split(['/', '\\']).any(|segment| segment == *name))
}

struct ParsedEmail {
    sent_at: Option<DateTime<Utc>>,
    received_at: Option<DateTime<Utc>>,
    from: String,
    to: Vec<String>,
    cc: Vec<String>,
    bcc: Vec<String>,
    reply_to: Option<String>,
    return_path: Option<String>,
    subject: String,
    message_id: String,
    in_reply_to: Option<String>,
    references: Vec<String>,
    attachments: Vec<String>,
    attachment_details: Vec<EmailAttachmentDto>,
    headers: Vec<EmailHeaderDto>,
    body_preview: String,
    body_plain: Option<String>,
    body_html: Option<String>,
    x_mailer: Option<String>,
    x_originating_ip: Option<String>,
    message_class: Option<String>,
}

fn parse_email_message(bytes: &[u8]) -> Result<ParsedEmail, String> {
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

fn build_body_preview(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(BODY_PREVIEW_MAX_LINES)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(BODY_PREVIEW_MAX_LEN)
        .collect()
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_PLAIN: &str = "\
From: Alice <alice@example.com>\n\
To: Bob <bob@example.com>\n\
Cc: Carol <carol@example.com>\n\
Subject: Hello\n\
Date: Mon, 16 Jun 2025 10:00:00 +0000\n\
Message-Id: <abc123@example.com>\n\
X-Mailer: TestMailer/1.0\n\
\n\
Hello Bob,\n\
This is a test message.\n";

    const MULTIPART_ATTACHMENT: &str = "\
From: sender@example.com\n\
To: recipient@example.com\n\
Subject: Document\n\
Date: Sun, 15 Jun 2025 14:00:00 +0200\n\
Content-Type: multipart/mixed; boundary=\"bound123\"\n\
\n\
--bound123\n\
Content-Type: text/plain\n\
\n\
Please find the document attached.\n\
--bound123\n\
Content-Type: application/octet-stream; name=\"data.bin\"\n\
Content-Disposition: attachment; filename=\"data.bin\"\n\
Content-Transfer-Encoding: base64\n\
\n\
SGVsbG8gV29ybGQh\n\
--bound123--\n";

    const HTML_ALTERNATIVE: &str = "\
From: a@example.com\n\
To: b@example.com\n\
Subject: HTML mail\n\
Date: Tue, 17 Jun 2025 08:00:00 +0000\n\
Content-Type: multipart/alternative; boundary=\"alt\"\n\
\n\
--alt\n\
Content-Type: text/plain\n\
\n\
Plain body.\n\
--alt\n\
Content-Type: text/html\n\
\n\
<html><body>HTML body</body></html>\n\
--alt--\n";

    const EMLX_SIZE_PREFIX: &str = "1234\nFrom: a@example.com\nTo: b@example.com\nSubject: Emlx\nDate: Wed, 18 Jun 2025 09:00:00 +0000\n\nBody.\n";

    const ENCODED_HEADERS: &str = "\
From: =?UTF-8?B?5p2O5aic?= <a@example.com>\n\
To: b@example.com\n\
Subject: =?UTF-8?Q?=E4=B8=AD=E6=96=87=E4=B8=BB=E9=A2=98?=\n\
Date: Thu, 19 Jun 2025 10:00:00 +0000\n\n\
Body.\n";

    const REPLY_THREAD: &str = "\
From: a@example.com\n\
To: b@example.com\n\
Subject: Re: Thread\n\
Date: Fri, 20 Jun 2025 11:00:00 +0000\n\
Message-Id: <msg2@example.com>\n\
In-Reply-To: <msg1@example.com>\n\
References: <msg1@example.com> <msg1.1@example.com>\n\
\n\
Reply body.\n";

    #[test]
    fn parses_simple_plain_email() {
        let parsed =
            parse_email_message(SIMPLE_PLAIN.as_bytes()).expect("should parse simple email");
        assert_eq!(parsed.from, "Alice <alice@example.com>");
        assert_eq!(parsed.to, vec!["Bob <bob@example.com>"]);
        assert_eq!(parsed.cc, vec!["Carol <carol@example.com>"]);
        assert_eq!(parsed.subject, "Hello");
        assert_eq!(parsed.message_id, "<abc123@example.com>");
        assert_eq!(parsed.x_mailer.as_deref(), Some("TestMailer/1.0"));
        assert!(parsed.body_plain.as_deref().unwrap().contains("Hello Bob"));
        assert!(parsed.body_preview.contains("Hello Bob"));
        assert!(parsed.sent_at.is_some());
        assert!(!parsed.headers.is_empty());
    }

    #[test]
    fn parses_multipart_attachment() {
        let parsed = parse_email_message(MULTIPART_ATTACHMENT.as_bytes())
            .expect("should parse multipart email");
        assert_eq!(parsed.attachments, vec!["data.bin"]);
        assert_eq!(parsed.attachment_details.len(), 1);
        let att = &parsed.attachment_details[0];
        assert_eq!(att.file_name, "data.bin");
        assert_eq!(att.size, Some(12));
        assert_eq!(att.mime_type.as_deref(), Some("application/octet-stream"));
        assert!(parsed
            .body_plain
            .as_deref()
            .unwrap()
            .contains("document attached"));
    }

    #[test]
    fn parses_html_alternative() {
        let parsed =
            parse_email_message(HTML_ALTERNATIVE.as_bytes()).expect("should parse html email");
        assert!(parsed.body_plain.as_deref().unwrap().contains("Plain body"));
        assert!(parsed
            .body_html
            .as_deref()
            .unwrap()
            .contains("<html><body>HTML body</body></html>"));
    }

    #[test]
    fn strips_emlx_size_prefix() {
        let parsed =
            parse_email_message(EMLX_SIZE_PREFIX.as_bytes()).expect("should parse emlx email");
        assert_eq!(parsed.subject, "Emlx");
        assert!(parsed
            .body_plain
            .as_deref()
            .unwrap_or("")
            .trim()
            .contains("Body."));
    }

    #[test]
    fn decodes_encoded_headers() {
        let parsed =
            parse_email_message(ENCODED_HEADERS.as_bytes()).expect("should parse encoded headers");
        assert_eq!(parsed.from, "李娜 <a@example.com>");
        assert_eq!(parsed.subject, "中文主题");
    }

    #[test]
    fn parses_thread_headers() {
        let parsed =
            parse_email_message(REPLY_THREAD.as_bytes()).expect("should parse reply thread");
        assert_eq!(parsed.in_reply_to.as_deref(), Some("<msg1@example.com>"));
        assert_eq!(
            parsed.references,
            vec!["msg1@example.com", "msg1.1@example.com"]
        );
    }

    /// Regression gate for the public-small synthetic email fixtures.
    #[test]
    fn public_small_email_fixtures_match_expected() {
        use serde_json::Value;
        use std::fs;
        use std::path::PathBuf;

        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let fixture_dir = manifest_dir
            .join("../../testdata/fixtures/public-small/email")
            .canonicalize()
            .expect("fixture dir exists");
        let expected_path = fixture_dir.join("expected.json");
        let expected: Value =
            serde_json::from_str(&fs::read_to_string(expected_path).unwrap()).unwrap();

        for sample in expected["samples"].as_array().unwrap() {
            let file_name = sample["file"].as_str().unwrap();
            let sample_type = sample["type"].as_str().unwrap_or("eml");
            let exp = &sample["expected"];
            let bytes = fs::read(fixture_dir.join(file_name)).unwrap();

            if sample_type == "mbox" {
                assert_mbox_fixture(&bytes, exp, file_name);
                continue;
            }
            if sample_type == "pst" || sample_type == "ost" {
                assert_pst_fixture(&bytes, exp, file_name, sample_type);
                continue;
            }

            let parsed = parse_email_message(&bytes)
                .unwrap_or_else(|err| panic!("{file_name} should parse: {err}"));

            if let Some(v) = exp["from"].as_str() {
                assert_eq!(parsed.from, v, "{file_name} from");
            }
            if let Some(v) = exp["fromContains"].as_str() {
                assert!(
                    parsed.from.contains(v),
                    "{file_name} from should contain {v}"
                );
            }
            assert_eq_str_vec(&parsed.to, &exp["to"], file_name, "to");
            assert_eq_str_vec(&parsed.cc, &exp["cc"], file_name, "cc");
            assert_eq_str_vec(&parsed.bcc, &exp["bcc"], file_name, "bcc");
            assert_opt_eq(&parsed.reply_to, &exp["replyTo"], file_name, "replyTo");
            assert_opt_eq(
                &parsed.return_path,
                &exp["returnPath"],
                file_name,
                "returnPath",
            );
            if let Some(v) = exp["subject"].as_str() {
                assert_eq!(parsed.subject, v, "{file_name} subject");
            }
            if let Some(v) = exp["subjectContains"].as_str() {
                assert!(
                    parsed.subject.contains(v),
                    "{file_name} subject should contain {v}"
                );
            }
            if let Some(v) = exp["messageId"].as_str() {
                assert_eq!(parsed.message_id, v, "{file_name} messageId");
            }
            assert_opt_eq(
                &parsed.in_reply_to,
                &exp["inReplyTo"],
                file_name,
                "inReplyTo",
            );
            assert_eq_str_vec(
                &parsed.references,
                &exp["references"],
                file_name,
                "references",
            );
            assert_eq_str_vec(
                &parsed.attachments,
                &exp["attachments"],
                file_name,
                "attachments",
            );
            assert_contains(
                parsed.body_preview.as_str(),
                &exp["bodyPreviewContains"],
                file_name,
                "bodyPreview",
            );
            assert_opt_contains(
                parsed.body_plain.as_deref(),
                &exp["bodyPlainContains"],
                file_name,
                "bodyPlain",
            );
            assert_opt_contains(
                parsed.body_html.as_deref(),
                &exp["bodyHtmlContains"],
                file_name,
                "bodyHtml",
            );
            assert_opt_eq(&parsed.x_mailer, &exp["xMailer"], file_name, "xMailer");
            assert_opt_contains(
                parsed.x_originating_ip.as_deref(),
                &exp["xOriginatingIp"],
                file_name,
                "xOriginatingIp",
            );

            if let Some(v) = exp["attachmentCount"].as_u64() {
                assert_eq!(
                    parsed.attachment_details.len() as u64,
                    v,
                    "{file_name} attachment count"
                );
            }
            if let Some(details) = exp["attachmentDetails"].as_array() {
                assert_eq!(
                    parsed.attachment_details.len(),
                    details.len(),
                    "{file_name} attachment details length"
                );
                for (actual, expected) in parsed.attachment_details.iter().zip(details.iter()) {
                    if let Some(v) = expected["fileName"].as_str() {
                        assert_eq!(actual.file_name, v, "attachment fileName");
                    }
                    if let Some(v) = expected["mimeType"].as_str() {
                        assert_eq!(actual.mime_type.as_deref(), Some(v), "attachment mimeType");
                    }
                    if let Some(v) = expected["size"].as_u64() {
                        assert_eq!(actual.size.unwrap_or(0), v, "attachment size");
                    }
                    assert_opt_eq(
                        &actual.content_id,
                        &expected["contentId"],
                        file_name,
                        "contentId",
                    );
                }
            }

            if let Some(v) = exp["sentAt"].as_str() {
                let expected_date = chrono::DateTime::parse_from_rfc3339(v)
                    .unwrap()
                    .with_timezone(&chrono::Utc);
                assert_eq!(parsed.sent_at.unwrap(), expected_date, "{file_name} sentAt");
            }
            assert_opt_eq(
                &parsed.message_class,
                &exp["messageClass"],
                file_name,
                "messageClass",
            );
        }
    }

    /// Regression gate for the public-medium synthetic email fixtures.
    #[test]
    fn public_medium_email_fixtures_match_expected() {
        use serde_json::Value;
        use std::fs;
        use std::path::PathBuf;

        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let fixture_dir = manifest_dir
            .join("../../testdata/fixtures/public-medium/email")
            .canonicalize()
            .expect("fixture dir exists");
        let expected_path = fixture_dir.join("expected.json");
        let expected: Value =
            serde_json::from_str(&fs::read_to_string(expected_path).unwrap()).unwrap();

        for sample in expected["samples"].as_array().unwrap() {
            let file_name = sample["file"].as_str().unwrap();
            let sample_type = sample["type"].as_str().unwrap_or("eml");
            let exp = &sample["expected"];
            let bytes = fs::read(fixture_dir.join(file_name)).unwrap();

            if sample_type == "mbox" {
                assert_mbox_fixture(&bytes, exp, file_name);
                continue;
            }
            if sample_type == "pst" || sample_type == "ost" {
                assert_pst_fixture(&bytes, exp, file_name, sample_type);
                continue;
            }

            let parsed = parse_email_message(&bytes)
                .unwrap_or_else(|err| panic!("{file_name} should parse: {err}"));

            if let Some(v) = exp["fromContains"].as_str() {
                assert!(
                    parsed.from.contains(v),
                    "{file_name} from should contain {v}"
                );
            }
            if let Some(v) = exp["subjectContains"].as_str() {
                assert!(
                    parsed.subject.contains(v),
                    "{file_name} subject should contain {v}, got {}",
                    parsed.subject
                );
            }
        }
    }

    fn assert_mbox_fixture(bytes: &[u8], exp: &Value, file_name: &str) {
        let candidate = EvidenceCandidate {
            file_id: domain::FileEntryId("file-mbox".to_string()),
            data_source_id: "ds-1".to_string(),
            path: format!("/fixtures/{file_name}"),
            size: bytes.len() as u64,
            evidence_kind: "email_mbox".to_string(),
            parser: "email.mbox".to_string(),
            category: "Email".to_string(),
        };
        let outcome = extract_mbox_candidate(&candidate, bytes);
        assert!(
            outcome.warnings.is_empty(),
            "{file_name} warnings: {:?}",
            outcome.warnings
        );

        if let Some(v) = exp["messagesCount"].as_u64() {
            assert_eq!(
                outcome.artifacts.len() as u64,
                v,
                "{file_name} artifact count"
            );
        }

        if let Some(expected_messages) = exp["messages"].as_array() {
            for (idx, (artifact, expected)) in outcome
                .artifacts
                .iter()
                .zip(expected_messages.iter())
                .enumerate()
            {
                let prefix = format!("{file_name} message {idx}");
                let attrs = &artifact.attrs;
                if let Some(v) = expected["from"].as_str() {
                    assert_eq!(string_attr(attrs, "from"), v, "{prefix} from");
                }
                if let Some(v) = expected["fromContains"].as_str() {
                    assert!(
                        string_attr(attrs, "from").contains(v),
                        "{prefix} from should contain {v}"
                    );
                }
                assert_eq_str_vec(
                    &string_vec_attr(attrs, "to"),
                    &expected["to"],
                    &prefix,
                    "to",
                );
                assert_eq_str_vec(
                    &string_vec_attr(attrs, "cc"),
                    &expected["cc"],
                    &prefix,
                    "cc",
                );
                assert_eq_str_vec(
                    &string_vec_attr(attrs, "bcc"),
                    &expected["bcc"],
                    &prefix,
                    "bcc",
                );
                assert_opt_eq(
                    &optional_string_attr(attrs, "replyTo"),
                    &expected["replyTo"],
                    &prefix,
                    "replyTo",
                );
                assert_opt_eq(
                    &optional_string_attr(attrs, "returnPath"),
                    &expected["returnPath"],
                    &prefix,
                    "returnPath",
                );
                if let Some(v) = expected["subject"].as_str() {
                    assert_eq!(string_attr(attrs, "subject"), v, "{prefix} subject");
                }
                assert_opt_eq(
                    &optional_string_attr(attrs, "messageId"),
                    &expected["messageId"],
                    &prefix,
                    "messageId",
                );
                assert_opt_eq(
                    &optional_string_attr(attrs, "inReplyTo"),
                    &expected["inReplyTo"],
                    &prefix,
                    "inReplyTo",
                );
                assert_eq_str_vec(
                    &string_vec_attr(attrs, "references"),
                    &expected["references"],
                    &prefix,
                    "references",
                );
                assert_contains(
                    &string_attr(attrs, "bodyPreview"),
                    &expected["bodyPreviewContains"],
                    &prefix,
                    "bodyPreview",
                );
                assert_opt_contains(
                    optional_string_attr(attrs, "bodyPlain").as_deref(),
                    &expected["bodyPlainContains"],
                    &prefix,
                    "bodyPlain",
                );
                assert_opt_contains(
                    optional_string_attr(attrs, "bodyHtml").as_deref(),
                    &expected["bodyHtmlContains"],
                    &prefix,
                    "bodyHtml",
                );
                assert_not_contains(
                    optional_string_attr(attrs, "bodyPlain")
                        .as_deref()
                        .unwrap_or(""),
                    &expected["bodyPlainNotContains"],
                    &prefix,
                    "bodyPlainNotContains",
                );
                assert_eq_str_vec(
                    &string_vec_attr(attrs, "attachments"),
                    &expected["attachments"],
                    &prefix,
                    "attachments",
                );
                assert_opt_contains(
                    optional_string_attr(attrs, "xMailer").as_deref(),
                    &expected["xMailer"],
                    &prefix,
                    "xMailer",
                );
                assert_opt_contains(
                    optional_string_attr(attrs, "xOriginatingIp").as_deref(),
                    &expected["xOriginatingIp"],
                    &prefix,
                    "xOriginatingIp",
                );
                if let Some(v) = expected["attachmentCount"].as_u64() {
                    assert_eq!(
                        attachment_details_attr(attrs, "attachmentDetails").len() as u64,
                        v,
                        "{prefix} attachment count"
                    );
                }
                if let Some(details) = expected["attachmentDetails"].as_array() {
                    let actual = attachment_details_attr(attrs, "attachmentDetails");
                    assert_eq!(
                        actual.len(),
                        details.len(),
                        "{prefix} attachment details length"
                    );
                    for (a, e) in actual.iter().zip(details.iter()) {
                        if let Some(v) = e["fileName"].as_str() {
                            assert_eq!(a.file_name, v, "attachment fileName");
                        }
                        if let Some(v) = e["mimeType"].as_str() {
                            assert_eq!(a.mime_type.as_deref(), Some(v), "attachment mimeType");
                        }
                        if let Some(v) = e["size"].as_u64() {
                            assert_eq!(a.size.unwrap_or(0), v, "attachment size");
                        }
                    }
                }
                if let Some(v) = expected["sentAt"].as_str() {
                    let expected_date = chrono::DateTime::parse_from_rfc3339(v)
                        .unwrap()
                        .with_timezone(&chrono::Utc);
                    let actual = optional_string_attr(attrs, "sentAt")
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&chrono::Utc));
                    assert_eq!(actual.unwrap(), expected_date, "{prefix} sentAt");
                }
                assert_opt_eq(
                    &optional_string_attr(attrs, "messageClass"),
                    &expected["messageClass"],
                    &prefix,
                    "messageClass",
                );
                if !expected["isDeleted"].is_null() {
                    assert_eq!(
                        bool_attr(attrs, "isDeleted"),
                        expected["isDeleted"].as_bool(),
                        "{prefix} isDeleted"
                    );
                }
            }
        }

        if let Some(first) = exp["firstMessage"].as_object() {
            if let Some(artifact) = outcome.artifacts.first() {
                assert_message_summary(
                    &artifact.attrs,
                    first,
                    &format!("{file_name} firstMessage"),
                );
            }
        }
        if let Some(last) = exp["lastMessage"].as_object() {
            if let Some(artifact) = outcome.artifacts.last() {
                assert_message_summary(&artifact.attrs, last, &format!("{file_name} lastMessage"));
            }
        }

        if let Some(v) = exp["containerPath"].as_str() {
            if let Some(first) = outcome.artifacts.first() {
                assert_eq!(
                    optional_string_attr(&first.attrs, "containerPath"),
                    Some(v.to_string()),
                    "{file_name} containerPath"
                );
            }
        }
    }

    fn assert_message_summary(
        attrs: &std::collections::BTreeMap<String, Value>,
        expected: &serde_json::Map<String, serde_json::Value>,
        prefix: &str,
    ) {
        if let Some(v) = expected.get("fromContains").and_then(Value::as_str) {
            assert!(
                string_attr(attrs, "from").contains(v),
                "{prefix} from should contain {v}"
            );
        }
        if let Some(v) = expected.get("subjectContains").and_then(Value::as_str) {
            assert!(
                string_attr(attrs, "subject").contains(v),
                "{prefix} subject should contain {v}"
            );
        }
        if let Some(v) = expected.get("bodyContains").and_then(Value::as_str) {
            assert!(
                string_attr(attrs, "bodyPreview").contains(v),
                "{prefix} bodyPreview should contain {v}"
            );
        }
        if let Some(v) = expected.get("bodyPlainContains").and_then(Value::as_str) {
            assert!(
                optional_string_attr(attrs, "bodyPlain")
                    .as_deref()
                    .unwrap_or("")
                    .contains(v),
                "{prefix} bodyPlain should contain {v}"
            );
        }
        assert_eq_str_vec(
            &string_vec_attr(attrs, "to"),
            &expected.get("to").cloned().unwrap_or(Value::Null),
            prefix,
            "to",
        );
        if let Some(v) = expected.get("attachmentCount").and_then(Value::as_u64) {
            assert_eq!(
                attachment_details_attr(attrs, "attachmentDetails").len() as u64,
                v,
                "{prefix} attachmentCount"
            );
        }
        assert_opt_eq(
            &optional_string_attr(attrs, "messageClass"),
            &expected.get("messageClass").cloned().unwrap_or(Value::Null),
            prefix,
            "messageClass",
        );
        if expected.get("isDeleted").is_some_and(|v| !v.is_null()) {
            assert_eq!(
                bool_attr(attrs, "isDeleted"),
                expected.get("isDeleted").and_then(Value::as_bool),
                "{prefix} isDeleted"
            );
        }
    }

    fn assert_pst_fixture(bytes: &[u8], exp: &Value, file_name: &str, sample_type: &str) {
        let candidate = EvidenceCandidate {
            file_id: domain::FileEntryId(format!("file-{sample_type}")),
            data_source_id: "ds-1".to_string(),
            path: format!("/fixtures/{file_name}"),
            size: bytes.len() as u64,
            evidence_kind: format!("email_{sample_type}"),
            parser: format!("email.{sample_type}"),
            category: "Email".to_string(),
        };
        let outcome = extract_pst_candidate(&candidate, bytes);
        assert!(
            outcome.warnings.is_empty(),
            "{file_name} warnings: {:?}",
            outcome.warnings
        );

        if let Some(v) = exp["messagesCount"].as_u64() {
            assert_eq!(
                outcome.artifacts.len() as u64,
                v,
                "{file_name} artifact count"
            );
        }

        if let Some(expected_messages) = exp["messages"].as_array() {
            for (idx, (artifact, expected)) in outcome
                .artifacts
                .iter()
                .zip(expected_messages.iter())
                .enumerate()
            {
                let prefix = format!("{file_name} message {idx}");
                let attrs = &artifact.attrs;
                if let Some(v) = expected["from"].as_str() {
                    assert_eq!(string_attr(attrs, "from"), v, "{prefix} from");
                }
                if let Some(v) = expected["fromContains"].as_str() {
                    assert!(
                        string_attr(attrs, "from").contains(v),
                        "{prefix} from should contain {v}"
                    );
                }
                assert_eq_str_vec(
                    &string_vec_attr(attrs, "to"),
                    &expected["to"],
                    &prefix,
                    "to",
                );
                assert_eq_str_vec(
                    &string_vec_attr(attrs, "cc"),
                    &expected["cc"],
                    &prefix,
                    "cc",
                );
                assert_eq_str_vec(
                    &string_vec_attr(attrs, "bcc"),
                    &expected["bcc"],
                    &prefix,
                    "bcc",
                );
                assert_opt_eq(
                    &optional_string_attr(attrs, "replyTo"),
                    &expected["replyTo"],
                    &prefix,
                    "replyTo",
                );
                assert_opt_eq(
                    &optional_string_attr(attrs, "returnPath"),
                    &expected["returnPath"],
                    &prefix,
                    "returnPath",
                );
                if let Some(v) = expected["subject"].as_str() {
                    assert_eq!(string_attr(attrs, "subject"), v, "{prefix} subject");
                }
                assert_opt_eq(
                    &optional_string_attr(attrs, "messageId"),
                    &expected["messageId"],
                    &prefix,
                    "messageId",
                );
                assert_opt_eq(
                    &optional_string_attr(attrs, "inReplyTo"),
                    &expected["inReplyTo"],
                    &prefix,
                    "inReplyTo",
                );
                assert_eq_str_vec(
                    &string_vec_attr(attrs, "references"),
                    &expected["references"],
                    &prefix,
                    "references",
                );
                assert_contains(
                    &string_attr(attrs, "bodyPreview"),
                    &expected["bodyPreviewContains"],
                    &prefix,
                    "bodyPreview",
                );
                assert_opt_contains(
                    optional_string_attr(attrs, "bodyPlain").as_deref(),
                    &expected["bodyPlainContains"],
                    &prefix,
                    "bodyPlain",
                );
                assert_opt_contains(
                    optional_string_attr(attrs, "bodyHtml").as_deref(),
                    &expected["bodyHtmlContains"],
                    &prefix,
                    "bodyHtml",
                );
                assert_opt_contains(
                    optional_string_attr(attrs, "xMailer").as_deref(),
                    &expected["xMailer"],
                    &prefix,
                    "xMailer",
                );
                assert_opt_contains(
                    optional_string_attr(attrs, "xOriginatingIp").as_deref(),
                    &expected["xOriginatingIp"],
                    &prefix,
                    "xOriginatingIp",
                );
                assert_eq_str_vec(
                    &string_vec_attr(attrs, "attachments"),
                    &expected["attachments"],
                    &prefix,
                    "attachments",
                );
                if let Some(v) = expected["attachmentCount"].as_u64() {
                    assert_eq!(
                        attachment_details_attr(attrs, "attachmentDetails").len() as u64,
                        v,
                        "{prefix} attachment count"
                    );
                }
                if let Some(v) = expected["sentAt"].as_str() {
                    let expected_date = chrono::DateTime::parse_from_rfc3339(v)
                        .unwrap()
                        .with_timezone(&chrono::Utc);
                    let actual = optional_string_attr(attrs, "sentAt")
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&chrono::Utc));
                    assert_eq!(actual.unwrap(), expected_date, "{prefix} sentAt");
                }
                assert_opt_eq(
                    &optional_string_attr(attrs, "messageClass"),
                    &expected["messageClass"],
                    &prefix,
                    "messageClass",
                );
                if !expected["isDeleted"].is_null() {
                    assert_eq!(
                        bool_attr(attrs, "isDeleted"),
                        expected["isDeleted"].as_bool(),
                        "{prefix} isDeleted"
                    );
                }
            }
        }

        if let Some(first) = exp["firstMessage"].as_object() {
            if let Some(artifact) = outcome.artifacts.first() {
                assert_message_summary(
                    &artifact.attrs,
                    first,
                    &format!("{file_name} firstMessage"),
                );
            }
        }
        if let Some(last) = exp["lastMessage"].as_object() {
            if let Some(artifact) = outcome.artifacts.last() {
                assert_message_summary(&artifact.attrs, last, &format!("{file_name} lastMessage"));
            }
        }

        if let Some(v) = exp["containerPath"].as_str() {
            if let Some(first) = outcome.artifacts.first() {
                assert_eq!(
                    optional_string_attr(&first.attrs, "containerPath"),
                    Some(v.to_string()),
                    "{file_name} containerPath"
                );
            }
        }
        if let Some(v) = exp["containerPathContains"].as_str() {
            if let Some(first) = outcome.artifacts.first() {
                let actual =
                    optional_string_attr(&first.attrs, "containerPath").unwrap_or_default();
                assert!(
                    actual.contains(v),
                    "{file_name} containerPath should contain {v}, got {actual}"
                );
            }
        }
    }

    fn assert_eq_str_vec(actual: &[String], expected: &Value, file_name: &str, field: &str) {
        if expected.is_null() {
            return;
        }
        let expected: Vec<String> = expected
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(actual.to_vec(), expected, "{file_name} {field}");
    }

    fn assert_opt_eq(actual: &Option<String>, expected: &Value, file_name: &str, field: &str) {
        if expected.is_null() {
            return;
        }
        if let Some(v) = expected.as_str() {
            assert_eq!(actual.as_deref(), Some(v), "{file_name} {field}");
        }
    }

    fn assert_opt_contains(actual: Option<&str>, expected: &Value, file_name: &str, field: &str) {
        if expected.is_null() {
            return;
        }
        if let Some(v) = expected.as_str() {
            let actual = actual.unwrap_or("");
            assert!(
                actual.contains(v),
                "{file_name} {field} should contain {v}, got {actual}"
            );
        }
    }

    fn assert_contains(actual: &str, expected: &Value, file_name: &str, field: &str) {
        if expected.is_null() {
            return;
        }
        if let Some(v) = expected.as_str() {
            assert!(
                actual.contains(v),
                "{file_name} {field} should contain {v}, got {actual}"
            );
        }
    }

    fn assert_not_contains(actual: &str, expected: &Value, file_name: &str, field: &str) {
        if expected.is_null() {
            return;
        }
        if let Some(v) = expected.as_str() {
            assert!(
                !actual.contains(v),
                "{file_name} {field} should NOT contain {v}, got {actual}"
            );
        }
    }

    fn string_attr(attrs: &std::collections::BTreeMap<String, Value>, key: &str) -> String {
        attrs
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    }

    fn bool_attr(attrs: &std::collections::BTreeMap<String, Value>, key: &str) -> Option<bool> {
        attrs.get(key).and_then(Value::as_bool)
    }

    fn optional_string_attr(
        attrs: &std::collections::BTreeMap<String, Value>,
        key: &str,
    ) -> Option<String> {
        attrs.get(key).and_then(Value::as_str).map(str::to_string)
    }

    fn string_vec_attr(
        attrs: &std::collections::BTreeMap<String, Value>,
        key: &str,
    ) -> Vec<String> {
        attrs
            .get(key)
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn attachment_details_attr(
        attrs: &std::collections::BTreeMap<String, Value>,
        key: &str,
    ) -> Vec<EmailAttachmentDto> {
        attrs
            .get(key)
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|v| {
                        Some(EmailAttachmentDto {
                            file_name: v.get("fileName")?.as_str()?.to_string(),
                            size: v.get("size")?.as_u64(),
                            mime_type: v.get("mimeType")?.as_str().map(str::to_string),
                            content_id: v.get("contentId")?.as_str().map(str::to_string),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
