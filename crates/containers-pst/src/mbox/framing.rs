use crate::{MboxMessage, PstError};
use mailparse::parse_headers;

use super::header::{
    find_header, parse_address, parse_email_date, parse_from_line, parse_message_ids,
    parse_recipients, split_headers_and_body, ParsedHeaderMap,
};
use super::mime::{parse_body_parts, unescape_from_lines};

/// Recognized mbox sub-format variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MboxVariant {
    /// mboxrd - "From " escaping via ">" prefix.
    MboxRd,
    /// mboxo - same escaping as mboxrd (classic Unix mbox).
    MboxO,
    /// mboxcl - Content-Length delimited.
    MboxCl,
    /// mboxcl2 - Content-Length delimited with extra metadata.
    MboxCl2,
}

/// Parse raw mbox data into a list of [`MboxMessage`]s.
///
/// Automatically detects the mbox variant. If the variant cannot be determined,
/// defaults to `mboxrd` style parsing.
pub fn parse_mbox(data: &[u8]) -> Result<Vec<MboxMessage>, PstError> {
    if data.is_empty() {
        return Ok(Vec::new());
    }

    let text = std::str::from_utf8(data)
        .map_err(|_| PstError::MboxError("mbox data is not valid UTF-8".to_string()))?;

    let variant = detect_variant(text);
    let raw_messages = split_into_raw_messages(text, variant);

    let mut messages = Vec::with_capacity(raw_messages.len());
    for raw in &raw_messages {
        if let Some(msg) = parse_single_message(raw, variant)? {
            messages.push(msg);
        }
    }

    Ok(messages)
}

/// Detect which mbox variant the data uses.
pub fn detect_variant(data: &str) -> MboxVariant {
    let first_chunk = &data[..data.len().min(8192)];
    let has_content_length = first_chunk
        .lines()
        .take(50)
        .any(|line| line.to_lowercase().starts_with("content-length:"));

    if has_content_length {
        let has_cl2_marker = first_chunk.lines().take(50).any(|line| {
            let lower = line.to_lowercase();
            lower.starts_with("status:") || lower.starts_with("x-status:")
        });
        if has_cl2_marker {
            return MboxVariant::MboxCl2;
        }
        return MboxVariant::MboxCl;
    }

    let has_escaped_from = data
        .lines()
        .any(|line| line.starts_with(">>From ") || line.starts_with(">From "));

    if has_escaped_from {
        return MboxVariant::MboxRd;
    }

    MboxVariant::MboxO
}

/// Split raw mbox text into per-message blocks, each starting with "From ".
pub(super) fn split_into_raw_messages(text: &str, _variant: MboxVariant) -> Vec<String> {
    let mut messages: Vec<String> = Vec::new();
    let mut current: Option<String> = None;

    for line in text.lines() {
        if is_message_separator(line) {
            if let Some(pending) = current.take() {
                messages.push(pending);
            }
            current = Some(String::new());
        }

        match &mut current {
            Some(buf) => {
                if !buf.is_empty() {
                    buf.push('\n');
                }
                buf.push_str(line);
            }
            None => {
                // Text before the first "From " line is ignored.
            }
        }
    }

    if let Some(pending) = current.take() {
        if !pending.trim().is_empty() {
            messages.push(pending);
        }
    }

    if messages.is_empty() {
        let mut alt: Vec<String> = Vec::new();
        let mut cur: Option<String> = None;
        for line in text.lines() {
            if line.starts_with("From ") {
                if let Some(p) = cur.take() {
                    alt.push(p);
                }
                cur = Some(String::new());
            }
            if let Some(ref mut buf) = cur {
                if !buf.is_empty() {
                    buf.push('\n');
                }
                buf.push_str(line);
            }
        }
        if let Some(p) = cur.take() {
            if !p.trim().is_empty() {
                alt.push(p);
            }
        }
        messages = alt;
    }

    messages
}

/// Returns `true` when `line` looks like an mbox "From " separator line.
pub(super) fn is_message_separator(line: &str) -> bool {
    if !line.starts_with("From ") {
        return false;
    }
    let parts: Vec<&str> = line.split_whitespace().collect();
    parts.len() >= 6
}

/// Parse one raw message block into an `MboxMessage`.
pub(super) fn parse_single_message(
    raw: &str,
    variant: MboxVariant,
) -> Result<Option<MboxMessage>, PstError> {
    let (mut header_str, body_str) = split_headers_and_body(raw);

    if header_str.is_empty() {
        if let Some(newline_pos) = raw.find('\n') {
            let after_from = &raw[newline_pos + 1..];
            let trimmed = after_from.trim();
            if !trimmed.is_empty() && trimmed.lines().any(|l| l.contains(':')) {
                header_str = after_from;
            } else if trimmed.is_empty() {
                return Ok(None);
            }
        } else {
            return Ok(None);
        }
    }

    if header_str.is_empty() {
        return Ok(None);
    }

    let body_str = match variant {
        MboxVariant::MboxRd | MboxVariant::MboxO => unescape_from_lines(body_str),
        MboxVariant::MboxCl | MboxVariant::MboxCl2 => body_str.to_string(),
    };

    let from_line = raw.lines().next().unwrap_or("");
    let from_addr = parse_from_line(from_line);

    let subject = find_header(header_str, "Subject").unwrap_or_default();
    let to_raw = find_header(header_str, "To").unwrap_or_default();
    let date_str = find_header(header_str, "Date").unwrap_or_default();
    let content_type =
        find_header(header_str, "Content-Type").unwrap_or_else(|| "text/plain".to_string());

    let (sender_name, sender_email) = parse_address(&from_addr);
    let recipients = parse_recipients(&to_raw);
    let sent_time = parse_email_date(&date_str);
    let received_time = None;
    let (body_plain, body_html, attachments) = parse_body_parts(&body_str, &content_type);

    let parsed_header_block = mailparse_header_block(header_str);
    let parsed_headers = parse_headers(parsed_header_block.as_bytes())
        .map(|(h, _)| h)
        .unwrap_or_default();
    let header_map = ParsedHeaderMap(&parsed_headers);

    let to = header_map.address_list("To");
    let cc = header_map.address_list("Cc");
    let bcc = header_map.address_list("Bcc");
    let reply_to = header_map.first_value("Reply-To");
    let return_path = header_map.first_value("Return-Path");
    let message_id = header_map.first_value("Message-Id");
    let in_reply_to = header_map.first_value("In-Reply-To");
    let references_raw = header_map.first_value("References");
    let references = if references_raw.is_empty() {
        Vec::new()
    } else {
        parse_message_ids(&references_raw)
    };
    let message_class = header_map.first_value("X-Message-Class");
    let x_mailer = header_map.first_value("X-Mailer");
    let x_originating_ip = header_map.first_value("X-Originating-IP");
    let headers = parsed_headers
        .iter()
        .map(|h| (h.get_key().to_string(), h.get_value().to_string()))
        .collect();

    Ok(Some(MboxMessage {
        subject,
        body_plain,
        body_html,
        sender_name,
        sender_email,
        recipients,
        to,
        cc,
        bcc,
        reply_to,
        return_path,
        message_id,
        in_reply_to,
        references,
        message_class,
        x_mailer,
        x_originating_ip,
        sent_time,
        received_time,
        attachments,
        folder_path: String::new(),
        headers,
    }))
}

fn mailparse_header_block(header_block: &str) -> &str {
    let Some(first_line) = header_block.lines().next() else {
        return header_block;
    };

    if first_line.starts_with("From ") && !first_line.starts_with("From:") {
        if let Some(pos) = header_block.find('\n') {
            return &header_block[pos + 1..];
        }
        return "";
    }

    header_block
}
