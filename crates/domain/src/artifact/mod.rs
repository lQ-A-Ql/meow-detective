use crate::FileEntryId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ArtifactId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactFamily {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: ArtifactId,
    pub family: String,
    pub title: String,
    pub summary: String,
    pub source_object_id: Option<FileEntryId>,
    pub extractor_id: Option<String>,
    pub extractor_version: Option<String>,
    pub confidence: Option<f32>,
    pub source_attribution: Option<String>,
    pub created_at: DateTime<Utc>,
    pub attrs: BTreeMap<String, Value>,
}
