use crate::PstAttachment;

use super::header::{find_header, split_headers_and_body};

/// Parse the body into plain text, HTML, and attachments based on Content-Type header.
pub(super) fn parse_body_parts(
    body: &str,
    content_type: &str,
) -> (String, String, Vec<PstAttachment>) {
    let ct_lower = content_type.to_lowercase();

    if ct_lower.starts_with("multipart/") {
        if let Some(boundary) = extract_boundary(content_type) {
            return parse_multipart(body, &boundary);
        }
    }

    if ct_lower.contains("text/html") {
        let (plain, _html) = strip_mime_headers(body);
        (String::new(), plain, Vec::new())
    } else {
        let (plain, _html) = strip_mime_headers(body);
        (plain, String::new(), Vec::new())
    }
}

/// Strip any intra-part MIME headers (Content-Type, Content-Transfer-Encoding, etc.)
pub(super) fn strip_mime_headers(part_body: &str) -> (String, String) {
    let (_, body) = split_headers_and_body(part_body);
    let body = body.trim().to_string();
    (body, String::new())
}

/// Extract the boundary parameter from a Content-Type header value.
pub(super) fn extract_boundary(content_type: &str) -> Option<String> {
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

/// Parse a multipart MIME body into plain text, HTML, and attachments.
pub(super) fn parse_multipart(body: &str, boundary: &str) -> (String, String, Vec<PstAttachment>) {
    let boundary_marker = format!("--{}", boundary);
    let end_marker = format!("--{}--", boundary);

    let mut plain = String::new();
    let mut html = String::new();
    let mut attachments: Vec<PstAttachment> = Vec::new();

    let mut in_part = false;
    let mut current_part = String::new();

    for line in body.lines() {
        let trimmed = line.trim();

        if trimmed == boundary_marker {
            if in_part && !current_part.trim().is_empty() {
                process_multipart_section(&current_part, &mut plain, &mut html, &mut attachments);
            }
            in_part = true;
            current_part = String::new();
            continue;
        }

        if trimmed == end_marker {
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
pub(super) fn process_multipart_section(
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
        let decoded = decode_body(body, &cte);
        let decoded_str = String::from_utf8_lossy(&decoded).to_string();
        if !plain.is_empty() {
            plain.push('\n');
        }
        plain.push_str(&decoded_str);
    }
}

/// Extract a filename from Content-Disposition or Content-Type.
pub(super) fn extract_filename(content_disposition: &str, content_type: &str) -> String {
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
pub(super) fn decode_body(body: &str, encoding: &str) -> Vec<u8> {
    let body = body.trim();
    let body = body.replace("=\r\n", "").replace("=\n", "");

    match encoding.to_lowercase().as_str() {
        "base64" => {
            let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(&compact)
                .unwrap_or_else(|_| body.as_bytes().to_vec())
        }
        "quoted-printable" => quoted_printable_decode(&body),
        "7bit" | "8bit" | "" => body.as_bytes().to_vec(),
        _ => body.as_bytes().to_vec(),
    }
}

/// Decode quoted-printable content.
pub(super) fn quoted_printable_decode(input: &str) -> Vec<u8> {
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

/// Unescape mboxrd/mboxo-style ">From " quoting.
pub(super) fn unescape_from_lines(body: &str) -> String {
    let mut result = String::with_capacity(body.len());
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix('>') {
            if rest.starts_with("From") || rest.starts_with('>') {
                result.push_str(rest);
                result.push('\n');
                continue;
            }
        }
        result.push_str(line);
        result.push('\n');
    }
    if !body.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }
    result
}
