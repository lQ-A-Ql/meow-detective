use serde::{Deserialize, Serialize};

use crate::dto::analysis_base::AnalysisParseStatusDto;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailExtractionSummaryDto {
    pub status: AnalysisParseStatusDto,
    pub total: u64,
    pub messages: Vec<EmailMessageDto>,
    pub accounts: Vec<EmailAccountDto>,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailAccountDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incoming_server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outgoing_server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incoming_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outgoing_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incoming_ssl: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outgoing_ssl: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_algorithm: Option<String>,
    pub mail_client: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailAttachmentDto {
    pub file_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailHeaderDto {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailMessageDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub received_at: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_path: Option<String>,
    pub subject: String,
    pub message_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub attachments: Vec<String>,
    pub attachment_details: Vec<EmailAttachmentDto>,
    pub headers: Vec<EmailHeaderDto>,
    pub body_preview: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_plain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_html: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_mailer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mail_client: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_originating_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_status: Option<String>,
    pub attachment_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_deleted: Option<bool>,
}
