use chrono::{DateTime, Utc};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageHeader {
    pub name: String,
    pub value: String,
    pub raw_value: Vec<u8>,
    pub range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageAttachment {
    pub name: Option<String>,
    pub mime_type: String,
    pub content_id: Option<String>,
    pub decoded_size: u64,
    pub body_range: Range<usize>,
    pub data: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Message {
    pub subject: String,
    pub from: String,
    pub sender_name: String,
    pub sender_email: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub reply_to: Option<String>,
    pub return_path: Option<String>,
    pub message_id: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub message_class: Option<String>,
    pub x_mailer: Option<String>,
    pub x_originating_ip: Option<String>,
    pub date_raw: Option<String>,
    pub sent_time: Option<DateTime<Utc>>,
    pub received_time: Option<DateTime<Utc>>,
    pub body_plain: Option<String>,
    pub body_html: Option<String>,
    pub attachments: Vec<MessageAttachment>,
    pub headers: Vec<MessageHeader>,
}
