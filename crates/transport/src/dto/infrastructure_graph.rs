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
}
