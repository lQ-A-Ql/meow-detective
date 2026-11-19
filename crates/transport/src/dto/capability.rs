use serde::{Deserialize, Serialize};

/// Capability families tracked per data source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityKindDto {
    Catalog,
    Search,
    Timeline,
    Graph,
    PlatformFacts,
    Artifacts,
}

/// Lifecycle state of one per-source capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityStateDto {
    Pending,
    Queued,
    Running,
    Ready,
    Partial,
    Deferred,
    Failed,
    Stale,
    Cancelled,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityProgressDto {
    pub done: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCapabilityDto {
    pub data_source_id: String,
    pub capability: CapabilityKindDto,
    pub state: CapabilityStateDto,
    pub revision: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<CapabilityProgressDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// True = derived from markers at read time, not task-confirmed.
    pub derived: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityMatrixDto {
    pub sources: Vec<SourceCapabilityDto>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/capability.rs"]
mod tests;
