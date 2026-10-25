use crate::{message::Message, MboxMessage, PstAttachment};

impl From<Message> for MboxMessage {
    fn from(message: Message) -> Self {
        Self {
            subject: message.subject,
            sender_name: message.sender_name,
            sender_email: message.sender_email,
            body_plain: message.body_plain.unwrap_or_default(),
            body_html: message.body_html.unwrap_or_default(),
            recipients: message.to.clone(),
            to: message.to,
            cc: message.cc,
            bcc: message.bcc,
            reply_to: message.reply_to.unwrap_or_default(),
            return_path: message.return_path.unwrap_or_default(),
            message_id: message.message_id,
            in_reply_to: message.in_reply_to.unwrap_or_default(),
            references: message.references,
            message_class: message.message_class.unwrap_or_default(),
            x_mailer: message.x_mailer.unwrap_or_default(),
            x_originating_ip: message.x_originating_ip.unwrap_or_default(),
            sent_time: message.sent_time,
            received_time: message.received_time,
            attachments: message
                .attachments
                .into_iter()
                .map(|attachment| PstAttachment {
                    name: attachment.name.unwrap_or_default(),
                    size: attachment.decoded_size,
                    mime_type: attachment.mime_type,
                    content_id: attachment.content_id,
                    data: attachment.data.unwrap_or_default(),
                })
                .collect(),
            headers: message
                .headers
                .into_iter()
                .map(|header| (header.name, header.value))
                .collect(),
            folder_path: String::new(),
        }
    }
}
