use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRowDto {
    pub id: String,
    pub artifact_type: String,
    pub title: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_object_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extractor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extractor_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_attribution: Option<String>,
    pub created_at: String,
    pub attrs: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyCountDto {
    pub family: String,
    pub count: u64,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/artifacts.rs"]
mod tests;
