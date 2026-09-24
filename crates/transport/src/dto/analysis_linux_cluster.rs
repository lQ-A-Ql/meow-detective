use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxEvidenceSetListItemDto {
    pub import_set_id: String,
    pub name: String,
    pub state: String,
    pub member_count: u32,
    pub ready_count: u32,
    pub failed_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxEvidenceSetSummaryDto {
    pub import_set_id: String,
    pub name: String,
    pub state: String,
    pub member_count: u32,
    pub ready_count: u32,
    pub failed_count: u32,
    pub capability_level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_schema_version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collected_at: Option<String>,
    pub members: Vec<LinuxEvidenceSetMemberSummaryDto>,
    pub scopes: Vec<LinuxTopologyScopeSummaryDto>,
    pub edges: Vec<LinuxTopologyEdgeSummaryDto>,
    pub derived_sources: Vec<LinuxDerivedSourceSummaryDto>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxEvidenceSetMemberSummaryDto {
    pub member_index: u32,
    pub data_source_id: Option<String>,
    pub source_name: String,
    pub source_path: String,
    pub source_kind: String,
    pub import_state: String,
    pub hash_status: String,
    pub provenance_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxTopologyScopeSummaryDto {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub identity_state: String,
    pub status: String,
    pub evidence_completeness: String,
    pub member_count: u32,
    pub member_source_ids: Vec<String>,
    pub member_roles: Vec<LinuxTopologyMemberSummaryDto>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxTopologyMemberSummaryDto {
    pub data_source_id: String,
    pub role: String,
    pub member_index: Option<u32>,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxTopologyEdgeSummaryDto {
    pub source_scope_id: String,
    pub target_scope_id: String,
    pub kind: String,
    pub confidence: String,
    pub provenance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxDerivedSourceSummaryDto {
    pub data_source_id: String,
    pub kind: String,
    pub import_state: String,
    pub provenance_status: String,
    pub source_path: String,
}
