use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ApplicationLogLevelDto {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ApplicationLogDirectionDto {
    Latest,
    Forward,
    Backward,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationLogFilterDto {
    pub text: String,
    pub target: String,
    pub case_id: String,
    pub data_source_id: String,
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationLogRequestDto {
    pub direction: ApplicationLogDirectionDto,
    pub cursors: Vec<String>,
    pub filter: ApplicationLogFilterDto,
    pub limit_per_level: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationLogRecordDto {
    pub session_id: String,
    pub sequence: u64,
    pub timestamp: String,
    pub level: ApplicationLogLevelDto,
    pub target: String,
    pub message: String,
    pub fields: BTreeMap<String, String>,
    pub thread_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_name: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationLogPageDto {
    pub level: ApplicationLogLevelDto,
    pub records: Vec<ApplicationLogRecordDto>,
    pub forward_cursor: String,
    pub backward_cursor: String,
    pub has_more: bool,
    pub gap: bool,
    pub emitted: u64,
    pub evicted: u64,
    pub truncated: u64,
    pub file_dropped: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationLogWriterDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    pub failed_records: u64,
    pub queued_records: u64,
    pub queued_bytes: u64,
    pub written_sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationLogSnapshotDto {
    pub session_id: String,
    pub upper_sequence: u64,
    pub pages: Vec<ApplicationLogPageDto>,
    pub writer: ApplicationLogWriterDto,
}
