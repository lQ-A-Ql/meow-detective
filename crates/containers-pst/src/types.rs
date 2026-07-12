use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PstMessage {
    pub subject: String,
    pub body_plain: String,
    pub body_html: String,
    pub sender_name: String,
    pub sender_email: String,
    pub recipients: Vec<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub reply_to: String,
    pub return_path: String,
    pub message_id: String,
    pub in_reply_to: String,
    pub references: Vec<String>,
    pub message_class: String,
    pub x_mailer: String,
    pub x_originating_ip: String,
    pub sent_time: Option<DateTime<Utc>>,
    pub received_time: Option<DateTime<Utc>>,
    pub attachments: Vec<PstAttachment>,
    pub folder_path: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PstAttachment {
    pub name: String,
    pub size: u64,
    pub content_id: Option<String>,
    pub mime_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PstFolder {
    pub name: String,
    pub parent_path: String,
    pub depth: u32,
    pub message_count: u64,
    pub subfolder_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PstCalendar {
    pub subject: String,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub location: String,
    pub attendees: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PstContact {
    pub name: String,
    pub email: String,
    pub phone: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MboxMessage {
    pub subject: String,
    pub body_plain: String,
    pub body_html: String,
    pub sender_name: String,
    pub sender_email: String,
    pub recipients: Vec<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub reply_to: String,
    pub return_path: String,
    pub message_id: String,
    pub in_reply_to: String,
    pub references: Vec<String>,
    pub message_class: String,
    pub x_mailer: String,
    pub x_originating_ip: String,
    pub sent_time: Option<DateTime<Utc>>,
    pub received_time: Option<DateTime<Utc>>,
    pub attachments: Vec<PstAttachment>,
    pub folder_path: String,
    pub headers: Vec<(String, String)>,
}
