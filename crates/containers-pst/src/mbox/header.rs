use chrono::DateTime;
use mailparse::{addrparse_header, msgidparse, MailAddr, MailHeader, MailHeaderMap};

/// Split the raw message into header block and body block.
pub(super) fn split_headers_and_body(raw: &str) -> (&str, &str) {
    if let Some(pos) = raw.find("\n\n") {
        (&raw[..pos], &raw[pos + 2..])
    } else if let Some(pos) = raw.find("\r\n\r\n") {
        (&raw[..pos], &raw[pos + 4..])
    } else {
        ("", raw)
    }
}

/// Find a header value by name (case-insensitive). Handles header folding.
pub(super) fn find_header(header_block: &str, name: &str) -> Option<String> {
    let lower_name = name.to_lowercase();
    let mut lines = header_block.lines();
    while let Some(line) = lines.next() {
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().to_lowercase() == lower_name {
                let mut val = value.trim().to_string();
                loop {
                    let next = lines.clone().next();
                    match next {
                        Some(cont) if cont.starts_with(' ') || cont.starts_with('\t') => {
                            let trimmed = cont.trim_start();
                            val.push(' ');
                            val.push_str(trimmed);
                            lines.next();
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

/// Extract the sender email address from the mbox "From " separator line.
pub(super) fn parse_from_line(from_line: &str) -> String {
    let stripped = from_line.strip_prefix("From ").unwrap_or(from_line);
    stripped.split_whitespace().next().unwrap_or("").to_string()
}

/// Parse an email address string like `"Name" <addr>` or just `addr`.
pub(super) fn parse_address(raw: &str) -> (String, String) {
    let raw = raw.trim();
    if raw.is_empty() {
        return (String::new(), String::new());
    }

    if let Some(open) = raw.rfind('<') {
        if let Some(close) = raw.rfind('>') {
            let email = raw[open + 1..close].trim().to_string();
            let name = raw[..open].trim().trim_matches('"').trim().to_string();
            return (name, email);
        }
    }

    (String::new(), raw.to_string())
}

/// Parse a comma-separated recipient list into individual addresses.
pub(super) fn parse_recipients(to_header: &str) -> Vec<String> {
    if to_header.trim().is_empty() {
        return Vec::new();
    }
    to_header
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub(super) struct ParsedHeaderMap<'a>(pub(super) &'a [MailHeader<'a>]);

impl ParsedHeaderMap<'_> {
    pub(super) fn first_value(&self, name: &str) -> String {
        self.0
            .get_first_value(name)
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    pub(super) fn address_list(&self, name: &str) -> Vec<String> {
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

pub(super) fn parse_message_ids(raw: &str) -> Vec<String> {
    match msgidparse(raw) {
        Ok(list) => list
            .iter()
            .map(|id| id.trim().trim_matches(|c| c == '<' || c == '>').to_string())
            .filter(|id| !id.is_empty())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Parse an RFC 2822 / RFC 5322 date string into a UTC datetime.
pub(super) fn parse_email_date(date_str: &str) -> Option<DateTime<chrono::Utc>> {
    let cleaned = date_str.trim();

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
