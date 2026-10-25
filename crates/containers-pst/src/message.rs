mod fields;
mod model;
mod parts;

use crate::PstError;
use mail_parser::{MessageParser, PartFlags};
pub use model::{Message, MessageAttachment, MessageHeader};

pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;

/// Parse one RFC 5322 message. Attachment payloads are decoded only when requested.
pub fn parse_message(bytes: &[u8], attachment_contents: bool) -> Result<Message, PstError> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(PstError::MessageLimit("input bytes"));
    }
    let parser = MessageParser::new()
        .max_depth(32)
        .max_parts(4096)
        .max_encoded_nesting(3);
    let parsed = parser
        .parse(bytes)
        .ok_or(PstError::Message("missing header block"))?;
    if parsed.headers().len() > 4096 {
        return Err(PstError::MessageLimit("header count"));
    }
    for part in parsed.parts() {
        if part.flags().contains(PartFlags::LIMIT_REACHED)
            || part.flags().contains(PartFlags::NESTING_LIMIT)
        {
            return Err(PstError::MessageLimit("MIME structure"));
        }
        if part.has_problems() {
            return Err(PstError::Message("incomplete MIME structure"));
        }
    }
    let mut message = fields::project(&parsed);
    parts::project(&parsed, attachment_contents, &mut message)?;
    Ok(message)
}

/// Bound the existing EMLX length framing before passing message bytes to MIME parsing.
pub fn emlx_payload(bytes: &[u8]) -> Result<(&[u8], usize), PstError> {
    let Some(newline) = bytes.iter().position(|byte| *byte == b'\n') else {
        return Err(PstError::Message("missing EMLX length"));
    };
    if newline > 32 {
        return Err(PstError::MessageLimit("EMLX length header"));
    }
    let length = std::str::from_utf8(&bytes[..newline])
        .map_err(|_| PstError::Message("invalid EMLX length"))?
        .trim()
        .parse::<usize>()
        .map_err(|_| PstError::Message("invalid EMLX length"))?;
    let start = newline + 1;
    let end = start
        .checked_add(length)
        .ok_or(PstError::MessageLimit("EMLX length"))?;
    Ok((
        bytes
            .get(start..end)
            .ok_or(PstError::Message("truncated EMLX message"))?,
        start,
    ))
}
