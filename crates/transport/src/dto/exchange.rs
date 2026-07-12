use serde::{Deserialize, Serialize};

/// Request DTO to trigger a STIX 2.1 bundle export for the open case.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StixExportRequestDto {
    /// If set, export only artifacts matching this artifact type filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_type_filter: Option<String>,
}

/// Result DTO returned after a STIX 2.1 bundle export completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StixExportResultDto {
    /// The raw STIX 2.1 bundle as a pretty-printed JSON string.
    pub json: String,
    /// Total number of STIX objects in the bundle.
    pub object_count: u64,
    /// Count of indicators created from correlation leads.
    pub indicator_count: u64,
    /// Count of observed-data objects created from artifacts / registry / email.
    pub observed_data_count: u64,
    /// Count of relationship objects.
    pub relationship_count: u64,
    /// ISO 8601 timestamp of when the export was generated.
    pub generated_at: String,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/exchange.rs"]
mod tests;
