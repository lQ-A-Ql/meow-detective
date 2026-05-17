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
    pub created_at: DateTime<Utc>,
    pub attrs: BTreeMap<String, Value>,
}
