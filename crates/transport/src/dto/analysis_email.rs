use serde::{Deserialize, Serialize};

use crate::dto::analysis_base::AnalysisParseStatusDto;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailExtractionSummaryDto {
    pub status: AnalysisParseStatusDto,
    pub total: u64,
    pub messages: Vec<EmailMessageDto>,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailMessageDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub message_id: String,
    pub attachments: Vec<String>,
    pub body_preview: String,
}
