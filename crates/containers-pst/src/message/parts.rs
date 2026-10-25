use super::{Message, MessageAttachment, MAX_DECODED_BYTES};
use crate::PstError;
use mail_parser::{MessagePart, PartKind};

pub(super) fn project(
    parsed: &mail_parser::Message<'_>,
    contents: bool,
    output: &mut Message,
) -> Result<(), PstError> {
    let mut bytes = 0usize;
    for part in parsed.text_body().chain(parsed.html_body()) {
        if !part.is_text() {
            continue;
        }
        let (text, problems) = part
            .text_checked()
            .ok_or(PstError::Message("text body is unavailable"))?;
        if !problems.is_empty() {
            return Err(PstError::Message("body decoding failed"));
        }
        bytes = bytes
            .checked_add(text.len())
            .ok_or(PstError::MessageLimit("decoded bytes"))?;
        if bytes > MAX_DECODED_BYTES {
            return Err(PstError::MessageLimit("decoded bytes"));
        }
        match part.kind() {
            PartKind::Text if output.body_plain.is_none() => {
                output.body_plain = Some(text.into_owned())
            }
            PartKind::Html if output.body_html.is_none() => {
                output.body_html = Some(text.into_owned())
            }
            _ => {}
        }
    }
    for part in parsed.attachments() {
        if !part.has_known_transfer_encoding() {
            return Err(PstError::Message("unknown attachment encoding"));
        }
        let size = part.decoded_len();
        if contents {
            bytes = bytes
                .checked_add(size)
                .ok_or(PstError::MessageLimit("decoded bytes"))?;
            if bytes > MAX_DECODED_BYTES {
                return Err(PstError::MessageLimit("decoded bytes"));
            }
        }
        let data = if contents {
            let (data, problems) = part.decoded_checked();
            if !problems.is_empty() {
                return Err(PstError::Message("attachment decoding failed"));
            }
            Some(data.into_owned())
        } else {
            None
        };
        output.attachments.push(MessageAttachment {
            name: part.attachment_name().map(str::to_string),
            mime_type: mime_type(&part),
            content_id: part.content_id().map(str::to_string),
            decoded_size: size as u64,
            body_range: part.offset_body() as usize..part.offset_end() as usize,
            data,
        });
    }
    Ok(())
}

fn mime_type(part: &MessagePart<'_>) -> String {
    part.content_type()
        .map(|kind| match kind.subtype() {
            Some(subtype) => format!("{}/{subtype}", kind.ctype()),
            None => kind.ctype().to_string(),
        })
        .unwrap_or_else(|| "application/octet-stream".to_string())
}
