use crate::{
    message::{parse_message, Message, MAX_MESSAGE_BYTES},
    PstError,
};
use io_mbox::{
    entry::MboxEntry,
    format::MboxFormat,
    scan::{MboxScanner, MboxScannerOptions},
};
use std::ops::Range;

pub struct MboxRecord {
    pub message: Message,
    pub source: MboxSource,
}

pub struct MboxSource {
    pub source_range: Range<u64>,
    pub envelope_sender: String,
    pub variant: MboxFormat,
    pub variant_inferred: bool,
    pub unescaped: bool,
}

pub fn visit_mbox(
    bytes: &[u8],
    contents: bool,
    mut visit: impl FnMut(MboxRecord) -> Result<(), PstError>,
) -> Result<(), PstError> {
    let mut scanner = MboxScanner::new(
        0,
        MboxScannerOptions {
            content_length: true,
            header: true,
        },
    );
    let mut count = 0usize;
    for chunk in bytes.chunks(64 * 1024) {
        for entry in scanner.feed(chunk) {
            count += 1;
            if count > 100_000 {
                return Err(PstError::MessageLimit("mbox messages"));
            }
            visit(record(bytes, entry, contents)?)?;
        }
    }
    for entry in scanner.finish() {
        count += 1;
        if count > 100_000 {
            return Err(PstError::MessageLimit("mbox messages"));
        }
        visit(record(bytes, entry, contents)?)?;
    }
    if count == 0 && !bytes.is_empty() {
        return Err(PstError::Message("mbox contains no recognized envelope"));
    }
    Ok(())
}

fn record(bytes: &[u8], entry: MboxEntry, contents: bool) -> Result<MboxRecord, PstError> {
    if entry.header_len > 256 * 1024 {
        return Err(PstError::MessageLimit("mbox header bytes"));
    }
    let end = entry
        .message_offset
        .checked_add(entry.len)
        .ok_or(PstError::MessageLimit("mbox range"))?;
    let raw = bytes
        .get(
            entry.message_offset as usize
                ..usize::try_from(end).map_err(|_| PstError::MessageLimit("mbox range"))?,
        )
        .ok_or(PstError::Message("mbox range exceeds input"))?;
    if raw.len() > MAX_MESSAGE_BYTES {
        return Err(PstError::MessageLimit("mbox message bytes"));
    }
    let variant = infer(raw)?;
    let unquoted = variant.unescape(raw);
    let unescaped = unquoted.len() != raw.len();
    let message = parse_message(&unquoted, contents)?;
    if let Some(header) = message
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("Content-Length"))
    {
        let length = header
            .value
            .parse::<u64>()
            .map_err(|_| PstError::Message("invalid Content-Length"))?;
        let root = mail_parser::MessageParser::new()
            .parse_headers(raw)
            .ok_or(PstError::Message("missing mbox headers"))?;
        let start = root
            .parts()
            .next()
            .ok_or(PstError::Message("missing mbox root part"))?
            .offset_body() as usize;
        if length != raw.len().saturating_sub(start) as u64 {
            return Err(PstError::Message("mbox Content-Length does not match body"));
        }
    }
    Ok(MboxRecord {
        message,
        source: MboxSource {
            source_range: entry.message_offset..end,
            envelope_sender: entry.sender,
            variant,
            variant_inferred: true,
            unescaped,
        },
    })
}

fn infer(raw: &[u8]) -> Result<MboxFormat, PstError> {
    let parsed = mail_parser::MessageParser::new()
        .parse_headers(raw)
        .ok_or(PstError::Message("missing mbox headers"))?;
    let headers = parsed.headers();
    if headers.contains("Content-Length") {
        Ok(
            if headers.contains("Status") || headers.contains("X-Status") {
                MboxFormat::MboxCl2
            } else {
                MboxFormat::MboxCl
            },
        )
    } else if MboxFormat::MboxRd.unescape(raw).len() != raw.len() {
        Ok(MboxFormat::MboxRd)
    } else {
        Ok(MboxFormat::MboxO)
    }
}

pub fn detect_variant(bytes: impl AsRef<[u8]>) -> Result<MboxFormat, PstError> {
    let bytes = bytes.as_ref();
    let mut scanner = MboxScanner::new(
        0,
        MboxScannerOptions {
            content_length: true,
            header: false,
        },
    );
    for chunk in bytes.chunks(64 * 1024) {
        if let Some(first) = scanner.feed(chunk).into_iter().next() {
            return infer_entry(bytes, &first);
        }
    }
    let first = scanner
        .finish()
        .into_iter()
        .next()
        .ok_or(PstError::Message("mbox contains no recognized envelope"))?;
    infer_entry(bytes, &first)
}

fn infer_entry(bytes: &[u8], first: &MboxEntry) -> Result<MboxFormat, PstError> {
    if first.header_len > 256 * 1024 || first.len > MAX_MESSAGE_BYTES as u64 {
        return Err(PstError::MessageLimit("mbox detection input"));
    }
    let end = first
        .message_offset
        .checked_add(first.len)
        .ok_or(PstError::MessageLimit("mbox range"))?;
    let raw = bytes
        .get(first.message_offset as usize..end as usize)
        .ok_or(PstError::Message("invalid mbox range"))?;
    infer(raw)
}
