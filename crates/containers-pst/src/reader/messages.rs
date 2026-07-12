use super::PstReader;
use crate::props::{
    PROP_TAG_ATTACH_DATA, PROP_TAG_ATTACH_LONG_FILENAME, PROP_TAG_ATTACH_MIME,
    PROP_TAG_ATTACH_SIZE, PROP_TAG_BODY, PROP_TAG_DELIVERY_TIME, PROP_TAG_DISPLAY_BCC,
    PROP_TAG_DISPLAY_CC, PROP_TAG_DISPLAY_TO, PROP_TAG_INTERNET_MESSAGE_ID,
    PROP_TAG_IN_REPLY_TO_ID, PROP_TAG_MESSAGE_CLASS, PROP_TAG_REFERENCES, PROP_TAG_SENDER_EMAIL,
    PROP_TAG_SENDER_NAME, PROP_TAG_SENT_TIME, PROP_TAG_SUBJECT,
};
use crate::{PstAttachment, PstError, PstMessage};

impl PstReader {
    pub fn read_messages(&self) -> Result<Vec<PstMessage>, PstError> {
        let mut messages = Vec::new();
        for folder in self.collect_folder_nids()? {
            let path = self.get_folder_path(folder);
            for nid in self.get_subnode_nids(folder)? {
                if !is_email_class(
                    self.get_property_string(nid, PROP_TAG_MESSAGE_CLASS)
                        .as_deref(),
                ) {
                    continue;
                }
                if let Ok(message) = self.read_message(nid, &path) {
                    messages.push(message);
                }
            }
        }
        Ok(messages)
    }

    fn read_message(&self, nid: u32, folder_path: &str) -> Result<PstMessage, PstError> {
        self.read_subnode_block(nid)
            .ok_or_else(|| PstError::InvalidFormat(format!("No data block for NID {nid:X}")))?;
        let fields = MessageFields::read(self, nid);
        let to = split_display_addresses(&fields.to);
        let cc = split_display_addresses(&fields.cc);
        let bcc = split_display_addresses(&fields.bcc);
        let references = fields
            .references
            .as_deref()
            .map(split_display_addresses)
            .unwrap_or_default();
        let headers = fields.headers(&references);
        Ok(PstMessage {
            subject: fields.subject.clone(),
            body_plain: fields.body_plain,
            body_html: fields.body_html,
            sender_name: fields.sender_name.clone(),
            sender_email: fields.sender_email.clone(),
            recipients: to.iter().chain(&cc).cloned().collect(),
            to,
            cc,
            bcc,
            reply_to: String::new(),
            return_path: String::new(),
            message_id: fields.message_id.clone(),
            in_reply_to: fields.in_reply_to.clone(),
            references: references.clone(),
            message_class: fields.message_class.clone(),
            x_mailer: String::new(),
            x_originating_ip: String::new(),
            sent_time: self.get_property_filetime(nid, PROP_TAG_SENT_TIME),
            received_time: self.get_property_filetime(nid, PROP_TAG_DELIVERY_TIME),
            attachments: self.read_attachments(nid),
            folder_path: folder_path.to_string(),
            headers,
        })
    }

    fn read_attachments(&self, message_nid: u32) -> Vec<PstAttachment> {
        self.nbt_cache
            .keys()
            .copied()
            .filter(|nid| *nid > message_nid && *nid <= message_nid + 1000)
            .filter(|nid| {
                self.get_property_string(*nid, PROP_TAG_MESSAGE_CLASS)
                    .as_deref()
                    == Some("IPM.Attachment")
            })
            .map(|nid| PstAttachment {
                name: self
                    .get_property_string(nid, PROP_TAG_ATTACH_LONG_FILENAME)
                    .or_else(|| self.get_property_string(nid, 0x3704))
                    .unwrap_or_else(|| "unnamed".to_string()),
                size: self
                    .get_property_string(nid, PROP_TAG_ATTACH_SIZE)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
                content_id: self.get_property_string(nid, 0x3712),
                mime_type: self
                    .get_property_string(nid, PROP_TAG_ATTACH_MIME)
                    .unwrap_or_else(|| "application/octet-stream".to_string()),
                data: self
                    .get_property_binary(nid, PROP_TAG_ATTACH_DATA)
                    .unwrap_or_default(),
            })
            .collect()
    }
}

struct MessageFields {
    subject: String,
    body_plain: String,
    body_html: String,
    sender_name: String,
    sender_email: String,
    to: String,
    cc: String,
    bcc: String,
    message_id: String,
    in_reply_to: String,
    references: Option<String>,
    message_class: String,
}

impl MessageFields {
    fn read(reader: &PstReader, nid: u32) -> Self {
        let string = |tag| reader.get_property_string(nid, tag).unwrap_or_default();
        Self {
            subject: string(PROP_TAG_SUBJECT),
            body_plain: string(PROP_TAG_BODY),
            body_html: string(0x1013),
            sender_name: string(PROP_TAG_SENDER_NAME),
            sender_email: string(PROP_TAG_SENDER_EMAIL),
            to: string(PROP_TAG_DISPLAY_TO),
            cc: string(PROP_TAG_DISPLAY_CC),
            bcc: string(PROP_TAG_DISPLAY_BCC),
            message_id: string(PROP_TAG_INTERNET_MESSAGE_ID),
            in_reply_to: string(PROP_TAG_IN_REPLY_TO_ID),
            references: reader.get_property_string(nid, PROP_TAG_REFERENCES),
            message_class: reader
                .get_property_string(nid, PROP_TAG_MESSAGE_CLASS)
                .unwrap_or_else(|| "IPM.Note".to_string()),
        }
    }

    fn headers(&self, references: &[String]) -> Vec<(String, String)> {
        let mut headers = Vec::new();
        push_header(&mut headers, "Subject", &self.subject);
        let from = match (self.sender_name.is_empty(), self.sender_email.is_empty()) {
            (_, true) => String::new(),
            (true, false) => self.sender_email.clone(),
            (false, false) => format!("{} <{}>", self.sender_name, self.sender_email),
        };
        push_header(&mut headers, "From", &from);
        push_header(&mut headers, "To", &self.to);
        push_header(&mut headers, "Cc", &self.cc);
        push_header(&mut headers, "Bcc", &self.bcc);
        push_header(&mut headers, "Message-Id", &self.message_id);
        push_header(&mut headers, "In-Reply-To", &self.in_reply_to);
        push_header(&mut headers, "References", &references.join(" "));
        push_header(&mut headers, "Message-Class", &self.message_class);
        headers
    }
}

fn push_header(headers: &mut Vec<(String, String)>, name: &str, value: &str) {
    if !value.is_empty() {
        headers.push((name.to_string(), value.to_string()));
    }
}

fn is_email_class(class: Option<&str>) -> bool {
    matches!(
        class,
        Some("IPM.Note" | "IPM.Note.SMIME" | "IPM.Note.SMIME.MultipartSigned")
    )
}

fn split_display_addresses(raw: &str) -> Vec<String> {
    raw.split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}
