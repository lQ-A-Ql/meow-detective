use super::super::shared::build_body_preview;
pub(super) use super::model::ParsedEmail;
use containers_pst::{
    message::{emlx_payload, parse_message},
    PstError,
};
use transport::dto::{EmailAttachmentDto, EmailHeaderDto};

pub(crate) fn parse_email_message(bytes: &[u8]) -> Result<ParsedEmail, PstError> {
    let first_line = bytes
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    let first_line = first_line.strip_suffix(b"\r").unwrap_or(first_line);
    let (payload, source_offset) =
        if !first_line.is_empty() && first_line.iter().all(u8::is_ascii_digit) {
            emlx_payload(bytes)?
        } else {
            (bytes, 0)
        };
    let message = parse_message(payload, false)?;
    let details: Vec<_> = message
        .attachments
        .iter()
        .map(|attachment| EmailAttachmentDto {
            file_name: attachment.name.clone().unwrap_or_else(|| "unnamed".into()),
            size: Some(attachment.decoded_size),
            mime_type: Some(attachment.mime_type.clone()),
            content_id: attachment.content_id.clone(),
        })
        .collect();
    Ok(ParsedEmail {
        sent_at: message.sent_time,
        received_at: message.received_time,
        from: message.from,
        to: message.to,
        cc: message.cc,
        bcc: message.bcc,
        reply_to: message.reply_to,
        return_path: message.return_path,
        subject: message.subject,
        message_id: message.message_id,
        in_reply_to: message.in_reply_to,
        references: message.references,
        attachments: details
            .iter()
            .map(|attachment| attachment.file_name.clone())
            .collect(),
        attachment_details: details,
        headers: message
            .headers
            .into_iter()
            .map(|header| EmailHeaderDto {
                name: header.name,
                value: header.value,
            })
            .collect(),
        body_preview: message
            .body_plain
            .as_ref()
            .or(message.body_html.as_ref())
            .map(|body| build_body_preview(body))
            .unwrap_or_default(),
        body_plain: message.body_plain,
        body_html: message.body_html,
        x_mailer: message.x_mailer,
        x_originating_ip: message.x_originating_ip,
        message_class: message.message_class,
        source_offset,
        source_length: payload.len(),
    })
}
