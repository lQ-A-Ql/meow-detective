use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfrastructureGraphNodeDto {
    pub id: String,
    pub domain: String,
    pub kind: String,
    pub name: String,
    pub status: String,
    pub confidence: String,
    pub provenance_json: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfrastructureGraphEdgeDto {
    pub source_id: String,
    pub target_id: String,
    pub relation_kind: String,
    pub confidence: String,
    pub provenance_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfrastructureGraphDto {
    pub nodes: Vec<InfrastructureGraphNodeDto>,
    pub edges: Vec<InfrastructureGraphEdgeDto>,
    pub network_facts: Vec<InfrastructureNetworkFactDto>,
    pub network_diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfrastructureNetworkFactDto {
    pub id: String,
    pub data_source_id: String,
    pub environment_object_id: String,
    pub file_id: String,
    pub source_path: String,
    pub line_number: u64,
    pub fact_kind: String,
    pub subject: String,
    pub value: String,
    pub assertion_kind: String,
    pub confidence: String,
    pub parser: String,
}
