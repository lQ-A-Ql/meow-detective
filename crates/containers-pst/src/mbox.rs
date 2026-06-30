//! Mbox format support.
//!
//! Mbox is a generic term for a family of related file formats used for
//! storing collections of email messages.
//!
//! Variant detection per RFC 4155:
//! - **mboxrd**: lines starting with "From " in body are escaped with ">" prefix.
//!   A line `>From ` originated as `From `; a line `>>From ` originated as `>From `, etc.
//! - **mboxo**: same escaping as mboxrd (the original "mbox" format from Unix V7).
//! - **mboxcl**: "Content-Length" header delimits each message; no escaping needed.
//! - **mboxcl2**: like mboxcl but with additional metadata headers.

use crate::{MboxMessage, PstAttachment, PstError};
use chrono::DateTime;
use mailparse::{addrparse_header, msgidparse, parse_headers, MailAddr, MailHeader, MailHeaderMap};

/// Recognized mbox sub-format variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MboxVariant {
    /// mboxrd — "From " escaping via ">" prefix.
    MboxRd,
    /// mboxo — same escaping as mboxrd (classic Unix mbox).
    MboxO,
    /// mboxcl — Content-Length delimited.
    MboxCl,
    /// mboxcl2 — Content-Length delimited with extra metadata.
    MboxCl2,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

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
    // Check for Content-Length headers (mboxcl / mboxcl2).
    // In mboxcl formats, each message starts with "From " followed by a
    // "Content-Length:" header very early in the message.
    let first_chunk = &data[..data.len().min(8192)];
    let has_content_length = first_chunk
        .lines()
        .take(50)
        .any(|line| line.to_lowercase().starts_with("content-length:"));

    if has_content_length {
        // Check for mboxcl2 additional metadata headers after Content-Length.
        // mboxcl2 often includes Status: and X-Status: alongside Content-Length.
        let has_cl2_marker = first_chunk.lines().take(50).any(|line| {
            let lower = line.to_lowercase();
            lower.starts_with("status:") || lower.starts_with("x-status:")
        });
        if has_cl2_marker {
            return MboxVariant::MboxCl2;
        }
        return MboxVariant::MboxCl;
    }

    // Check for mboxrd-style escaped ">From " in message bodies.
    // mboxrd uses ">From " escaping for lines that start with "From ".
    let has_escaped_from = data
        .lines()
        .any(|line| line.starts_with(">>From ") || line.starts_with(">From "));

    if has_escaped_from {
        return MboxVariant::MboxRd;
    }

    // Default to mboxo (the original format). mboxo and mboxrd use the same
    // escaping convention — the only difference is that mboxrd documents the
    // ">" prefix escaping explicitly, while mboxo implementations vary.
    MboxVariant::MboxO
}

// ---------------------------------------------------------------------------
// Message splitting
// ---------------------------------------------------------------------------

/// Split raw mbox text into per-message blocks, each starting with "From ".
fn split_into_raw_messages(text: &str, _variant: MboxVariant) -> Vec<String> {
    let mut messages: Vec<String> = Vec::new();
    let mut current: Option<String> = None;

    for line in text.lines() {
        // A "From " line at the start of a message must match the separator
        // pattern: "From " followed by an email-ish token and a date.
        // We use a relaxed check here: line starts with "From " and is not
        // inside the body escaping (a raw "From " at line start in body would
        // be escaped as ">From " in mboxrd/mboxo, or the format is
        // Content-Length delimited so we don't need to worry).
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
                // Text before the first "From " line is ignored (mailbox
                // preamble — rare but valid).
            }
        }
    }

    if let Some(pending) = current.take() {
        if !pending.trim().is_empty() {
            messages.push(pending);
        }
    }

    // If variant is mboxcl / mboxcl2 and we found zero messages via "From "
    // splitting, fall back to splitting on "From " anywhere.
    if messages.is_empty() {
        // Last-resort: scan for any "From " line.
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
///
/// An mbox separator follows the pattern:
/// `From <sender> <weekday> <month> <day> <time> <year>`
fn is_message_separator(line: &str) -> bool {
    if !line.starts_with("From ") {
        return false;
    }
    // Check that there are enough space-separated tokens to form the date.
    let parts: Vec<&str> = line.split_whitespace().collect();
    // "From", sender (1 token), weekday, month, day, time, year = at least 6
    parts.len() >= 6
}

// ---------------------------------------------------------------------------
// Single message parsing
// ---------------------------------------------------------------------------

/// Parse one raw message block into an `MboxMessage`.
fn parse_single_message(raw: &str, variant: MboxVariant) -> Result<Option<MboxMessage>, PstError> {
    // Split headers from body at the first blank line.
    let (mut header_str, body_str) = split_headers_and_body(raw);

    // Edge case: no blank line found. If the text after the "From " separator
    // line contains colon-delimited tokens, treat them as headers with no body.
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

    // Unescape ">From " patterns in mboxrd/mboxo body.
    let body_str = match variant {
        MboxVariant::MboxRd | MboxVariant::MboxO => unescape_from_lines(body_str),
        MboxVariant::MboxCl | MboxVariant::MboxCl2 => body_str.to_string(),
    };

    // Parse main headers.
    let from_line = raw.lines().next().unwrap_or("");
    let from_addr = parse_from_line(from_line);

    let subject = find_header(header_str, "Subject").unwrap_or_default();
    let to_raw = find_header(header_str, "To").unwrap_or_default();
    let date_str = find_header(header_str, "Date").unwrap_or_default();
    let content_type =
        find_header(header_str, "Content-Type").unwrap_or_else(|| "text/plain".to_string());

    // Parse the "From" header into name + email.
    let (sender_name, sender_email) = parse_address(&from_addr);

    // Parse recipients from the To header for backward compatibility.
    let recipients = parse_recipients(&to_raw);

    // Parse date.
    let sent_time = parse_email_date(&date_str);
    let received_time = None; // Mbox does not carry a separate received time header.

    // Handle body based on Content-Type.
    let (body_plain, body_html, attachments) = parse_body_parts(&body_str, &content_type);

    // Use mailparse for complete RFC 5322/MIME header decoding.
    let parsed_headers = parse_headers(header_str.as_bytes())
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

// ---------------------------------------------------------------------------
// Header parsing helpers
// ---------------------------------------------------------------------------

/// Split the raw message into header block and body block.
/// Returns (header_text, body_text).
fn split_headers_and_body(raw: &str) -> (&str, &str) {
    // The first blank line separates headers from body.
    // Headers can be folded (continued on next line with leading whitespace).
    if let Some(pos) = raw.find("\n\n") {
        (&raw[..pos], &raw[pos + 2..])
    } else if let Some(pos) = raw.find("\r\n\r\n") {
        (&raw[..pos], &raw[pos + 4..])
    } else {
        // No blank line found — treat the whole thing as the body.
        ("", raw)
    }
}

/// Find a header value by name (case-insensitive). Handles header folding.
fn find_header(header_block: &str, name: &str) -> Option<String> {
    let lower_name = name.to_lowercase();
    let mut lines = header_block.lines();
    while let Some(line) = lines.next() {
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().to_lowercase() == lower_name {
                let mut val = value.trim().to_string();
                // Absorb folded continuation lines.
                loop {
                    let next = lines.clone().next();
                    match next {
                        Some(cont) if cont.starts_with(' ') || cont.starts_with('\t') => {
                            // Unfold: replace leading whitespace with a single space.
                            let trimmed = cont.trim_start();
                            val.push(' ');
                            val.push_str(trimmed);
                            lines.next(); // consume
                        }
                        _ => break,
                    }
                }
                return Some(val);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// "From " line parsing
// ---------------------------------------------------------------------------

/// Extract the sender email address from the mbox "From " separator line.
fn parse_from_line(from_line: &str) -> String {
    // Pattern: "From sender@example.com Day Mon DD HH:MM:SS YYYY"
    let stripped = from_line.strip_prefix("From ").unwrap_or(from_line);
    // The first token after "From " is the sender.
    stripped.split_whitespace().next().unwrap_or("").to_string()
}

// ---------------------------------------------------------------------------
// Address parsing
// ---------------------------------------------------------------------------

/// Parse an email address string like `"Name" <addr>` or just `addr`.
fn parse_address(raw: &str) -> (String, String) {
    let raw = raw.trim();
    if raw.is_empty() {
        return (String::new(), String::new());
    }

    // Try "Name <addr>" form.
    if let Some(open) = raw.rfind('<') {
        if let Some(close) = raw.rfind('>') {
            let email = raw[open + 1..close].trim().to_string();
            let name = raw[..open].trim().trim_matches('"').trim().to_string();
            return (name, email);
        }
    }

    // Plain address.
    (String::new(), raw.to_string())
}

/// Parse a comma-separated recipient list into individual addresses.
fn parse_recipients(to_header: &str) -> Vec<String> {
    if to_header.trim().is_empty() {
        return Vec::new();
    }
    to_header
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// mailparse-based header helpers
// ---------------------------------------------------------------------------

struct ParsedHeaderMap<'a>(&'a [MailHeader<'a>]);

impl ParsedHeaderMap<'_> {
    fn first_value(&self, name: &str) -> String {
        self.0
            .get_first_value(name)
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    fn address_list(&self, name: &str) -> Vec<String> {
        let Some(header) = self.0.get_first_header(name) else {
            return Vec::new();
        };
        match addrparse_header(header) {
            Ok(list) => list.into_inner().into_iter().map(format_address).collect(),
            Err(_) => Vec::new(),
        }
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

fn parse_message_ids(raw: &str) -> Vec<String> {
    match msgidparse(raw) {
        Ok(list) => list
            .iter()
            .map(|id| id.trim().trim_matches(|c| c == '<' || c == '>').to_string())
            .filter(|id| !id.is_empty())
            .collect(),
        Err(_) => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Date parsing
// ---------------------------------------------------------------------------

/// Parse an RFC 2822 / RFC 5322 date string into a UTC datetime.
fn parse_email_date(date_str: &str) -> Option<DateTime<chrono::Utc>> {
    let cleaned = date_str.trim();

    // Remove day-of-week prefix and trailing timezone comment.
    // "Wed, 14 Jun 2026 10:30:00 +0200"
    // "14 Jun 2026 10:30:00 +0200"
    // "Wed, 14 Jun 2026 10:30:00 +0200 (CEST)"

    // Try standard formats.
    let formats = [
        "%a, %d %b %Y %H:%M:%S %z",
        "%d %b %Y %H:%M:%S %z",
        "%a, %d %b %Y %H:%M:%S",
        "%d %b %Y %H:%M:%S",
        "%a, %d %b %y %H:%M:%S %z",
        "%d %b %y %H:%M:%S %z",
    ];

    for fmt in &formats {
        if let Ok(dt) = chrono::DateTime::parse_from_str(cleaned, fmt) {
            return Some(dt.with_timezone(&chrono::Utc));
        }
    }

    // Try without timezone — assume UTC.
    let formats_no_tz = [
        "%a, %d %b %Y %H:%M:%S",
        "%d %b %Y %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
    ];
    for fmt in &formats_no_tz {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(cleaned, fmt) {
            return Some(DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Body and MIME parsing
// ---------------------------------------------------------------------------

/// Parse the body into plain text, HTML, and attachments based on Content-Type header.
fn parse_body_parts(body: &str, content_type: &str) -> (String, String, Vec<PstAttachment>) {
    let ct_lower = content_type.to_lowercase();

    if ct_lower.starts_with("multipart/") {
        let boundary = extract_boundary(content_type);
        if let Some(boundary) = boundary {
            return parse_multipart(body, &boundary);
        }
    }

    // Single part — treat as plain text or HTML.
    if ct_lower.contains("text/html") {
        let (plain, _html) = strip_mime_headers(body);
        (String::new(), plain, Vec::new())
    } else {
        let (plain, _html) = strip_mime_headers(body);
        (plain, String::new(), Vec::new())
    }
}

/// Strip any intra-part MIME headers (Content-Type, Content-Transfer-Encoding, etc.)
/// from a single-part body. Returns (raw_body, optional_html).
fn strip_mime_headers(part_body: &str) -> (String, String) {
    // If the part has its own header block, skip it.
    let (_, body) = split_headers_and_body(part_body);
    let body = body.trim().to_string();
    (body, String::new())
}

/// Extract the boundary parameter from a Content-Type header value.
fn extract_boundary(content_type: &str) -> Option<String> {
    for part in content_type.split(';') {
        let trimmed = part.trim();
        if let Some(eq) = trimmed.find('=') {
            let key = trimmed[..eq].trim().to_lowercase();
            if key == "boundary" {
                let val = trimmed[eq + 1..].trim().trim_matches('"');
                return Some(val.to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Multipart parsing
// ---------------------------------------------------------------------------

/// Parse a multipart MIME body into plain text, HTML, and attachments.
fn parse_multipart(body: &str, boundary: &str) -> (String, String, Vec<PstAttachment>) {
    let boundary_marker = format!("--{}", boundary);
    let end_marker = format!("--{}--", boundary);

    let mut plain = String::new();
    let mut html = String::new();
    let mut attachments: Vec<PstAttachment> = Vec::new();

    // Split on boundary lines.
    let mut in_part = false;
    let mut current_part = String::new();

    for line in body.lines() {
        let trimmed = line.trim();

        if trimmed == boundary_marker {
            // Start of a new part — process the previous one.
            if in_part && !current_part.trim().is_empty() {
                process_multipart_section(&current_part, &mut plain, &mut html, &mut attachments);
            }
            in_part = true;
            current_part = String::new();
            continue;
        }

        if trimmed == end_marker {
            // End of all parts.
            if in_part && !current_part.trim().is_empty() {
                process_multipart_section(&current_part, &mut plain, &mut html, &mut attachments);
            }
            break;
        }

        if in_part {
            if !current_part.is_empty() {
                current_part.push('\n');
            }
            current_part.push_str(line);
        }
    }

    (plain, html, attachments)
}

/// Process one section (part) of a multipart message.
fn process_multipart_section(
    section: &str,
    plain: &mut String,
    html: &mut String,
    attachments: &mut Vec<PstAttachment>,
) {
    let (headers, body) = split_headers_and_body(section);
    let ct = find_header(headers, "Content-Type").unwrap_or_else(|| "text/plain".to_string());
    let ct_lower = ct.to_lowercase();
    let cd = find_header(headers, "Content-Disposition").unwrap_or_default();
    let cte = find_header(headers, "Content-Transfer-Encoding").unwrap_or_default();
    let cid = find_header(headers, "Content-ID");

    let is_attachment = cd.to_lowercase().contains("attachment")
        || (!cd.to_lowercase().contains("inline") && !ct_lower.starts_with("text/"));

    // Handle nested multipart.
    if ct_lower.starts_with("multipart/") {
        if let Some(inner_boundary) = extract_boundary(&ct) {
            let (p, h, a) = parse_multipart(body, &inner_boundary);
            if !p.is_empty() {
                if !plain.is_empty() {
                    plain.push('\n');
                }
                plain.push_str(&p);
            }
            if !h.is_empty() {
                if !html.is_empty() {
                    html.push('\n');
                }
                html.push_str(&h);
            }
            attachments.extend(a);
        }
    } else if is_attachment {
        // Extract attachment.
        let filename = extract_filename(&cd, &ct);
        let data = decode_body(body, &cte);
        let mime_type = ct.split(';').next().unwrap_or(&ct).trim().to_string();

        attachments.push(PstAttachment {
            name: filename,
            size: data.len() as u64,
            content_id: cid.map(|c| c.trim().trim_matches('<').trim_matches('>').to_string()),
            mime_type,
            data,
        });
    } else if ct_lower.starts_with("text/html") {
        let decoded = decode_body(body, &cte);
        let decoded_str = String::from_utf8_lossy(&decoded).to_string();
        if !html.is_empty() {
            html.push('\n');
        }
        html.push_str(&decoded_str);
    } else {
        // Default to plain text.
        let decoded = decode_body(body, &cte);
        let decoded_str = String::from_utf8_lossy(&decoded).to_string();
        if !plain.is_empty() {
            plain.push('\n');
        }
        plain.push_str(&decoded_str);
    }
}

/// Extract a filename from Content-Disposition or Content-Type.
fn extract_filename(content_disposition: &str, content_type: &str) -> String {
    // Try Content-Disposition: attachment; filename="name"
    for part in content_disposition.split(';') {
        let trimmed = part.trim();
        if let Some(eq) = trimmed.find('=') {
            let key = trimmed[..eq].trim().to_lowercase();
            if key == "filename" {
                let val = trimmed[eq + 1..].trim().trim_matches('"');
                return val.to_string();
            }
        }
    }
    // Fall back to Content-Type: ...; name="name"
    for part in content_type.split(';') {
        let trimmed = part.trim();
        if let Some(eq) = trimmed.find('=') {
            let key = trimmed[..eq].trim().to_lowercase();
            if key == "name" {
                let val = trimmed[eq + 1..].trim().trim_matches('"');
                return val.to_string();
            }
        }
    }
    "unnamed".to_string()
}

/// Decode a body based on Content-Transfer-Encoding.
fn decode_body(body: &str, encoding: &str) -> Vec<u8> {
    let body = body.trim();
    // Decode quoted-printable soft line breaks.
    let body = body.replace("=\r\n", "").replace("=\n", "");

    match encoding.to_lowercase().as_str() {
        "base64" => {
            // Strip whitespace before decoding.
            let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(&compact)
                .unwrap_or_else(|_| body.as_bytes().to_vec())
        }
        "quoted-printable" => quoted_printable_decode(&body),
        "7bit" | "8bit" | "" => body.as_bytes().to_vec(),
        _ => {
            // Unknown encoding — return raw.
            body.as_bytes().to_vec()
        }
    }
}

/// Decode quoted-printable content.
fn quoted_printable_decode(input: &str) -> Vec<u8> {
    let mut output: Vec<u8> = Vec::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '=' && i + 2 < chars.len() {
            let hex = format!("{}{}", chars[i + 1], chars[i + 2]);
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                output.push(byte);
                i += 3;
                continue;
            }
        }
        output.push(chars[i] as u8);
        i += 1;
    }
    output
}

// ---------------------------------------------------------------------------
// ">From " unescaping
// ---------------------------------------------------------------------------

/// Unescape mboxrd/mboxo-style ">From " quoting.
///
/// In mboxrd, a line that originally started with "From " is escaped by
/// prepending ">". A line that originally started with ">From " becomes
/// ">>From ", and so on.
fn unescape_from_lines(body: &str) -> String {
    let mut result = String::with_capacity(body.len());
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix('>') {
            // Strip one level of ">" quoting if the remainder starts with
            // "From" (covers both "From " separator lines and "From:" headers)
            // or another ">" (for multi-level escaping).
            if rest.starts_with("From") || rest.starts_with('>') {
                result.push_str(rest);
                result.push('\n');
                continue;
            }
        }
        result.push_str(line);
        result.push('\n');
    }
    // Remove trailing newline if the original didn't have one.
    if !body.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal mbox with a single message.
    const SAMPLE_SINGLE: &str = "\
From alice@example.com Fri Jun 13 10:00:00 2025
From: Alice <alice@example.com>
To: Bob <bob@example.com>
Subject: Hello
Date: Fri, 13 Jun 2025 10:00:00 +0000
Content-Type: text/plain

Hello Bob,
This is a test message.
Best,
Alice
";

    /// Two messages in mboxrd format, with ">From " escaping in the body.
    const SAMPLE_MBOXRD: &str = "\
From alice@example.com Mon Jun 16 09:00:00 2025
From: Alice <alice@example.com>
To: Bob <bob@example.com>
Subject: Forwarded note
Date: Mon, 16 Jun 2025 09:00:00 +0200
Content-Type: text/plain

FYI — see below.

>From charlie@example.com Mon Jun 16 08:00:00 2025
>From: Charlie <charlie@example.com>
>To: Alice <alice@example.com>
>Subject: Original
>
>Original message content here.

From bob@example.com Mon Jun 16 10:00:00 2025
From: Bob <bob@example.com>
To: Alice <alice@example.com>
Subject: Re: Forwarded note
Date: Mon, 16 Jun 2025 10:00:00 +0200
Content-Type: text/plain

Got it, thanks!
";

    /// A message with a multipart body containing an attachment.
    const SAMPLE_MULTIPART: &str = "\
From sender@example.com Sun Jun 15 14:00:00 2025
From: Sender <sender@example.com>
To: Recipient <recipient@example.com>
Subject: Document with attachment
Date: Sun, 15 Jun 2025 14:00:00 +0200
Content-Type: multipart/mixed; boundary=\"----boundary123\"

------boundary123
Content-Type: text/plain

Please find the document attached.

------boundary123
Content-Type: application/octet-stream; name=\"data.bin\"
Content-Disposition: attachment; filename=\"data.bin\"
Content-Transfer-Encoding: base64

SGVsbG8gV29ybGQh

------boundary123--
";

    /// mboxcl format with Content-Length headers.
    const SAMPLE_MBOXCL: &str = "\
From sender@example.com Mon Jun 16 12:00:00 2025
Content-Length: 120
From: Sender <sender@example.com>
To: Recipient <recipient@example.com>
Subject: CL test
Date: Mon, 16 Jun 2025 12:00:00 +0200

This message uses Content-Length.

From sender2@example.com Mon Jun 16 13:00:00 2025
Content-Length: 110
From: Sender2 <sender2@example.com>
To: Recipient <recipient@example.com>
Subject: Second CL test
Date: Mon, 16 Jun 2025 13:00:00 +0200

Another Content-Length message.
";

    /// mboxcl2 format with Content-Length and Status/X-Status headers.
    const SAMPLE_MBOXCL2: &str = "\
From sender@example.com Mon Jun 16 14:00:00 2025
Content-Length: 145
Status: RO
X-Status: F
From: Sender <sender@example.com>
To: Recipient <recipient@example.com>
Subject: CL2 test
Date: Mon, 16 Jun 2025 14:00:00 +0200

This message uses Content-Length with Status headers.
";

    /// Empty mbox.
    const SAMPLE_EMPTY: &str = "";

    // -----------------------------------------------------------------------

    #[test]
    fn test_detect_variant_mboxrd() {
        let v = detect_variant(SAMPLE_MBOXRD);
        assert_eq!(v, MboxVariant::MboxRd);
    }

    #[test]
    fn test_detect_variant_mboxo() {
        let v = detect_variant(SAMPLE_SINGLE);
        assert_eq!(v, MboxVariant::MboxO);
    }

    #[test]
    fn test_detect_variant_mboxcl() {
        let v = detect_variant(SAMPLE_MBOXCL);
        assert_eq!(v, MboxVariant::MboxCl);
    }

    #[test]
    fn test_detect_variant_mboxcl2() {
        let v = detect_variant(SAMPLE_MBOXCL2);
        assert_eq!(v, MboxVariant::MboxCl2);
    }

    #[test]
    fn test_parse_single_message() {
        let messages = parse_mbox(SAMPLE_SINGLE.as_bytes()).expect("parse should succeed");
        assert_eq!(messages.len(), 1);

        let msg = &messages[0];
        assert_eq!(msg.subject, "Hello");
        assert_eq!(msg.sender_email, "alice@example.com");
        assert_eq!(msg.recipients.len(), 1);
        assert_eq!(msg.recipients[0], "Bob <bob@example.com>");
        assert!(msg.body_plain.contains("Hello Bob"));
        assert!(msg.body_plain.contains("This is a test message."));
        assert!(msg.sent_time.is_some());
        assert_eq!(msg.attachments.len(), 0);
    }

    #[test]
    fn test_parse_mboxrd_unescaping() {
        let messages = parse_mbox(SAMPLE_MBOXRD.as_bytes()).expect("parse should succeed");
        assert_eq!(messages.len(), 2);

        let first = &messages[0];
        assert_eq!(first.subject, "Forwarded note");
        // The body should NOT contain literal ">From " after unescaping.
        assert!(!first.body_plain.contains(">From "));
        assert!(!first.body_plain.contains(">From:"));
        // The original escaped content should appear without the leading ">".
        assert!(first.body_plain.contains("From charlie@example.com"));
        assert!(first.body_plain.contains("Original message content here."));

        let second = &messages[1];
        assert_eq!(second.subject, "Re: Forwarded note");
        assert!(second.body_plain.contains("Got it, thanks!"));
    }

    #[test]
    fn test_parse_multipart_with_attachment() {
        let messages = parse_mbox(SAMPLE_MULTIPART.as_bytes()).expect("parse should succeed");
        assert_eq!(messages.len(), 1);

        let msg = &messages[0];
        assert_eq!(msg.subject, "Document with attachment");
        assert!(msg
            .body_plain
            .contains("Please find the document attached."));
        assert_eq!(msg.attachments.len(), 1);

        let att = &msg.attachments[0];
        assert_eq!(att.name, "data.bin");
        assert_eq!(att.mime_type, "application/octet-stream");
        assert_eq!(att.data, b"Hello World!");
        assert_eq!(att.size, 12);
    }

    #[test]
    fn test_parse_mboxcl() {
        let messages = parse_mbox(SAMPLE_MBOXCL.as_bytes()).expect("parse should succeed");
        assert_eq!(messages.len(), 2);

        assert_eq!(messages[0].subject, "CL test");
        assert_eq!(messages[1].subject, "Second CL test");
    }

    #[test]
    fn test_parse_mboxcl2() {
        let messages = parse_mbox(SAMPLE_MBOXCL2.as_bytes()).expect("parse should succeed");
        assert_eq!(messages.len(), 1);

        let msg = &messages[0];
        assert_eq!(msg.subject, "CL2 test");
        assert_eq!(msg.sender_email, "sender@example.com");
    }

    #[test]
    fn test_parse_empty() {
        let messages = parse_mbox(SAMPLE_EMPTY.as_bytes()).expect("parse should succeed");
        assert_eq!(messages.len(), 0);
    }

    #[test]
    fn test_parse_address_name_and_email() {
        let (name, email) = parse_address("\"Alice Johnson\" <alice@example.com>");
        assert_eq!(name, "Alice Johnson");
        assert_eq!(email, "alice@example.com");
    }

    #[test]
    fn test_parse_address_email_only() {
        let (name, email) = parse_address("bob@example.com");
        assert_eq!(name, "");
        assert_eq!(email, "bob@example.com");
    }

    #[test]
    fn test_unescape_from_lines_no_escape() {
        let input = "Hello\nWorld\n";
        let output = unescape_from_lines(input);
        assert_eq!(output, "Hello\nWorld\n");
    }

    #[test]
    fn test_unescape_from_lines_single_level() {
        let input = "Line 1\n>From alice@example.com Mon Jun 16 10:00:00 2025\nLine 3\n";
        let output = unescape_from_lines(input);
        assert_eq!(
            output,
            "Line 1\nFrom alice@example.com Mon Jun 16 10:00:00 2025\nLine 3\n"
        );
    }

    #[test]
    fn test_unescape_from_lines_double_escape() {
        // ">>From " means the original was ">From ".
        let input = ">>From alice@example.com Mon Jun 16 10:00:00 2025\n";
        let output = unescape_from_lines(input);
        assert_eq!(output, ">From alice@example.com Mon Jun 16 10:00:00 2025\n");
    }

    #[test]
    fn test_unescape_from_lines_non_from_greater_than() {
        // A ">" that doesn't precede "From " should be left alone.
        let input = "> This is a quote\n>From escaped\n";
        let output = unescape_from_lines(input);
        assert_eq!(output, "> This is a quote\nFrom escaped\n");
    }

    #[test]
    fn test_is_message_separator_valid() {
        assert!(is_message_separator(
            "From alice@example.com Fri Jun 13 10:00:00 2025"
        ));
    }

    #[test]
    fn test_is_message_separator_invalid() {
        // Too few tokens.
        assert!(!is_message_separator("From alice@example.com"));
        // Not starting with "From ".
        assert!(!is_message_separator("From: Alice <alice@example.com>"));
        // In-body "From " that is part of normal text (has only 2 tokens).
        assert!(!is_message_separator("From the beginning"));
    }

    #[test]
    fn test_split_headers_and_body() {
        let raw = "From: alice@example.com\nSubject: Test\n\nBody text here\n";
        let (headers, body) = split_headers_and_body(raw);
        assert!(headers.contains("From:"));
        assert!(headers.contains("Subject:"));
        assert_eq!(body.trim(), "Body text here");
    }

    #[test]
    fn test_find_header() {
        let headers = "From: Alice <alice@example.com>\nSubject: Test\nTo: Bob <bob@example.com>";
        assert_eq!(find_header(headers, "Subject").unwrap(), "Test");
        assert_eq!(find_header(headers, "To").unwrap(), "Bob <bob@example.com>");
        assert!(find_header(headers, "X-Unknown").is_none());
    }

    #[test]
    fn test_find_header_folded() {
        let headers = "Subject: This is a very long subject\n line that continues\nTo: Bob";
        let val = find_header(headers, "Subject").unwrap();
        assert_eq!(val, "This is a very long subject line that continues");
    }

    #[test]
    fn test_quoted_printable_decode() {
        let input = "Hello=20World=21";
        let output = quoted_printable_decode(input);
        assert_eq!(String::from_utf8_lossy(&output), "Hello World!");
    }

    #[test]
    fn test_extract_boundary() {
        let ct = r#"multipart/mixed; boundary="----=_NextPart_001""#;
        let b = extract_boundary(ct).unwrap();
        assert_eq!(b, "----=_NextPart_001");
    }

    #[test]
    fn test_extract_boundary_no_quotes() {
        let ct = "multipart/alternative; boundary=boundary123";
        let b = extract_boundary(ct).unwrap();
        assert_eq!(b, "boundary123");
    }

    #[test]
    fn test_extract_filename_from_disposition() {
        let cd = r#"attachment; filename="report.pdf""#;
        let ct = "application/pdf";
        let name = extract_filename(cd, ct);
        assert_eq!(name, "report.pdf");
    }

    #[test]
    fn test_extract_filename_from_content_type() {
        let cd = "inline";
        let ct = r#"application/octet-stream; name="data.bin""#;
        let name = extract_filename(cd, ct);
        assert_eq!(name, "data.bin");
    }

    #[test]
    fn test_parse_mbox_returns_mbox_message_struct() {
        let messages = parse_mbox(SAMPLE_SINGLE.as_bytes()).expect("parse should succeed");
        let msg = &messages[0];

        // Verify all MboxMessage fields are populated sensibly.
        assert!(!msg.subject.is_empty());
        assert!(!msg.body_plain.is_empty());
        // body_html may be empty for plain-text messages.
        assert!(msg.body_html.is_empty());
        assert!(!msg.sender_name.is_empty() || !msg.sender_email.is_empty());
        assert!(!msg.recipients.is_empty());
        assert!(msg.sent_time.is_some());
        assert!(msg.received_time.is_none()); // mbox doesn't have separate received time
        assert_eq!(msg.attachments.len(), 0);
        assert_eq!(msg.folder_path, ""); // mbox is flat
    }

    #[test]
    fn test_parse_mbox_handles_html_content() {
        let sample = "\
From sender@example.com Mon Jun 16 15:00:00 2025
From: Sender <sender@example.com>
To: Recipient <recipient@example.com>
Subject: HTML test
Date: Mon, 16 Jun 2025 15:00:00 +0200
Content-Type: text/html

<html><body><h1>Hello</h1><p>HTML message</p></body></html>
";
        let messages = parse_mbox(sample.as_bytes()).expect("parse should succeed");
        let msg = &messages[0];
        assert!(!msg.body_html.is_empty());
        assert!(msg.body_html.contains("<h1>Hello</h1>"));
    }

    #[test]
    fn test_parse_mbox_handles_multipart_alternative() {
        let sample = "\
From sender@example.com Mon Jun 16 16:00:00 2025
From: Sender <sender@example.com>
To: Recipient <recipient@example.com>
Subject: Multipart alternative
Date: Mon, 16 Jun 2025 16:00:00 +0200
Content-Type: multipart/alternative; boundary=altboundary

--altboundary
Content-Type: text/plain

Plain text version.

--altboundary
Content-Type: text/html

<html><body><p>HTML version.</p></body></html>

--altboundary--
";
        let messages = parse_mbox(sample.as_bytes()).expect("parse should succeed");
        let msg = &messages[0];
        assert!(msg.body_plain.contains("Plain text version."));
        assert!(msg.body_html.contains("<p>HTML version.</p>"));
    }

    #[test]
    fn test_parse_mbox_handles_no_blank_line_separator() {
        // Edge case: header block with no trailing blank line.
        let sample = "From sender@example.com Mon Jun 16 17:00:00 2025\nSubject: Minimal";
        let messages = parse_mbox(sample.as_bytes()).expect("parse should succeed");
        // Should still produce one message even with no body.
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].subject, "Minimal");
    }

    #[test]
    fn test_parse_mbox_handles_crlf() {
        let sample = "\
From sender@example.com Mon Jun 16 18:00:00 2025\r\n\
From: Sender <sender@example.com>\r\n\
To: Recipient <recipient@example.com>\r\n\
Subject: CRLF test\r\n\
Date: Mon, 16 Jun 2025 18:00:00 +0200\r\n\
\r\n\
Body with CRLF line endings.\r\n\
";
        let messages = parse_mbox(sample.as_bytes()).expect("parse should succeed");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].subject, "CRLF test");
        assert!(messages[0]
            .body_plain
            .contains("Body with CRLF line endings."));
    }

    #[test]
    fn test_variant_detection_handles_no_escape_no_cl() {
        // A message without escapes and without Content-Length should be mboxo.
        let v = detect_variant(SAMPLE_SINGLE);
        assert_eq!(v, MboxVariant::MboxO);
    }

    #[test]
    fn test_parse_address_with_angle_brackets_but_no_name() {
        let (name, email) = parse_address("<alice@example.com>");
        assert_eq!(email, "alice@example.com");
        assert_eq!(name, "");
    }

    #[test]
    fn test_parse_date_iso_format() {
        let date = parse_email_date("2025-06-16T10:30:00");
        assert!(date.is_some());
    }

    #[test]
    fn test_parse_date_invalid_returns_none() {
        let date = parse_email_date("not a date");
        assert!(date.is_none());
    }
}
