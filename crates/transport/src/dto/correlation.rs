use serde::{Deserialize, Serialize};

use super::analysis::{CorrelationFamilyCoverageDto, VerificationGuaranteeLevelDto};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum CorrelationConfidenceDto {
    Direct,
    Strong,
    Weak,
    Heuristic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum CorrelationNodeKindDto {
    File,
    Artifact,
    TimelineEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum CorrelationEdgeKindDto {
    SourceReference,
    SharedSourceObject,
    TemporalContext,
    PathMatch,
    NameMatch,
    RecoveredOriginalPath,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct CorrelationJumpTargetDto {
    pub route: String,
    pub target_id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CorrelationProvenanceDto {
    pub source_kind: String,
    pub source_record_id: String,
    pub source_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer_version: Option<String>,
    pub guarantee_level: VerificationGuaranteeLevelDto,
    pub warning_summary: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CorrelationNodeDto {
    pub id: String,
    pub kind: CorrelationNodeKindDto,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_object_id: Option<String>,
    pub related_count: u32,
    pub badges: Vec<String>,
    pub jumps: Vec<CorrelationJumpTargetDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CorrelationEdgeDto {
    pub id: String,
    pub kind: CorrelationEdgeKindDto,
    pub from_node_id: String,
    pub to_node_id: String,
    pub summary: String,
    pub confidence: CorrelationConfidenceDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CorrelationClusterDto {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub confidence: CorrelationConfidenceDto,
    pub families: Vec<String>,
    pub primary_file_id: String,
    pub artifact_count: u32,
    pub timeline_count: u32,
    pub node_ids: Vec<String>,
    pub edge_ids: Vec<String>,
    pub provenance: Vec<CorrelationProvenanceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CorrelationLeadDto {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub confidence: CorrelationConfidenceDto,
    pub families: Vec<String>,
    pub primary_file_id: String,
    pub supporting_node_ids: Vec<String>,
    pub match_signals: Vec<String>,
    pub jumps: Vec<CorrelationJumpTargetDto>,
    pub provenance: Vec<CorrelationProvenanceDto>,
    pub caveats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CorrelationSnapshotDto {
    pub generated_at: String,
    pub node_count: u32,
    pub edge_count: u32,
    pub cluster_count: u32,
    pub lead_count: u32,
    pub family_coverage: Vec<CorrelationFamilyCoverageDto>,
    pub nodes: Vec<CorrelationNodeDto>,
    pub edges: Vec<CorrelationEdgeDto>,
    pub clusters: Vec<CorrelationClusterDto>,
    pub leads: Vec<CorrelationLeadDto>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/correlation.rs"]
mod tests;
