use serde::{Deserialize, Serialize};

use crate::dto::analysis_base::AnalysisParseStatusDto;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesClusterSummaryDto {
    pub status: AnalysisParseStatusDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_name: Option<String>,
    pub selected_data_source_id: String,
    pub expected_member_count: u32,
    pub ready_member_count: u32,
    pub control_plane_member_count: u32,
    pub artifact_count: u64,
    pub nodes: Vec<KubernetesClusterNodeDto>,
    pub artifacts: Vec<KubernetesClusterArtifactDto>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesClusterNodeDto {
    pub data_source_id: String,
    pub source_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    pub control_plane: bool,
    pub artifact_count: u64,
    pub parsed_artifact_count: u64,
    pub failed_artifact_count: u64,
    pub status: AnalysisParseStatusDto,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesClusterArtifactDto {
    pub data_source_id: String,
    pub file_id: String,
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub deleted: bool,
    pub encrypted: bool,
    pub status: AnalysisParseStatusDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub diagnostics: Vec<String>,
}
