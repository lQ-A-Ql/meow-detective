use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxEvidenceEventDto {
    pub event_id: String,
    pub data_source_id: String,
    pub source_object_id: String,
    pub event_type: String,
    pub event_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingest_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_skew_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_sequence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    pub completeness: String,
    pub title: String,
    pub description: String,
}
