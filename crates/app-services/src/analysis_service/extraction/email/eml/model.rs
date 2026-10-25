use chrono::{DateTime, Utc};
use transport::dto::{EmailAttachmentDto, EmailHeaderDto};

pub(crate) struct ParsedEmail {
    pub(crate) sent_at: Option<DateTime<Utc>>,
    pub(crate) received_at: Option<DateTime<Utc>>,
    pub(crate) from: String,
    pub(crate) to: Vec<String>,
    pub(crate) cc: Vec<String>,
    pub(crate) bcc: Vec<String>,
    pub(crate) reply_to: Option<String>,
    pub(crate) return_path: Option<String>,
    pub(crate) subject: String,
    pub(crate) message_id: String,
    pub(crate) in_reply_to: Option<String>,
    pub(crate) references: Vec<String>,
    pub(crate) attachments: Vec<String>,
    pub(crate) attachment_details: Vec<EmailAttachmentDto>,
    pub(crate) headers: Vec<EmailHeaderDto>,
    pub(crate) body_preview: String,
    pub(crate) body_plain: Option<String>,
    pub(crate) body_html: Option<String>,
    pub(crate) x_mailer: Option<String>,
    pub(crate) x_originating_ip: Option<String>,
    pub(crate) message_class: Option<String>,
    pub(crate) source_offset: usize,
    pub(crate) source_length: usize,
}
