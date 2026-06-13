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
mod tests {
    use super::*;
    use crate::dto::analysis::CorrelationCoverageStatusDto;

    #[test]
    fn correlation_snapshot_serializes_camel_case_contract() {
        let snapshot = CorrelationSnapshotDto {
            generated_at: "2026-06-12T00:00:00Z".to_string(),
            node_count: 3,
            edge_count: 2,
            cluster_count: 1,
            lead_count: 1,
            family_coverage: vec![CorrelationFamilyCoverageDto {
                family: "LNK".to_string(),
                display_name: "LNK".to_string(),
                status: CorrelationCoverageStatusDto::Covered,
                lead_count: 1,
                high_confidence_lead_count: 1,
                review_lead_count: 0,
                cluster_count: 1,
                sample_signals: vec!["LNK 目标路径命中文件路径".to_string()],
            }],
            nodes: vec![CorrelationNodeDto {
                id: "file:file-1".to_string(),
                kind: CorrelationNodeKindDto::File,
                title: "cmd.exe".to_string(),
                subtitle: Some("C:/Windows/System32/cmd.exe".to_string()),
                source_object_id: Some("file-1".to_string()),
                related_count: 2,
                badges: vec!["deleted".to_string()],
                jumps: vec![CorrelationJumpTargetDto {
                    route: "/files".to_string(),
                    target_id: "file-1".to_string(),
                    label: "定位文件".to_string(),
                }],
            }],
            edges: vec![CorrelationEdgeDto {
                id: "edge-1".to_string(),
                kind: CorrelationEdgeKindDto::PathMatch,
                from_node_id: "artifact:1".to_string(),
                to_node_id: "file:file-1".to_string(),
                summary: "LNK 目标路径命中文件路径".to_string(),
                confidence: CorrelationConfidenceDto::Direct,
            }],
            clusters: vec![CorrelationClusterDto {
                id: "cluster:file-1".to_string(),
                title: "cmd.exe".to_string(),
                summary: "围绕该文件形成 1 条路径类命中。".to_string(),
                confidence: CorrelationConfidenceDto::Direct,
                families: vec!["LNK".to_string()],
                primary_file_id: "file-1".to_string(),
                artifact_count: 1,
                timeline_count: 1,
                node_ids: vec!["file:file-1".to_string()],
                edge_ids: vec!["edge-1".to_string()],
                provenance: vec![CorrelationProvenanceDto {
                    source_kind: "artifact".to_string(),
                    source_record_id: "artifact-1".to_string(),
                    source_label: "LNK".to_string(),
                    producer: Some("lnk".to_string()),
                    producer_version: Some("1.0.0".to_string()),
                    guarantee_level: VerificationGuaranteeLevelDto::BestEffort,
                    warning_summary: Vec::new(),
                }],
            }],
            leads: vec![CorrelationLeadDto {
                id: "lead:file-1".to_string(),
                title: "cmd.exe 形成关联线索".to_string(),
                summary: "LNK 目标路径命中文件路径。".to_string(),
                confidence: CorrelationConfidenceDto::Direct,
                families: vec!["LNK".to_string()],
                primary_file_id: "file-1".to_string(),
                supporting_node_ids: vec!["artifact:1".to_string(), "timeline:1".to_string()],
                match_signals: vec![
                    "LNK 目标路径命中文件路径".to_string(),
                    "关联文件时间线事件提供上下文".to_string(),
                ],
                jumps: vec![CorrelationJumpTargetDto {
                    route: "/timeline".to_string(),
                    target_id: "timeline-1".to_string(),
                    label: "打开时间线".to_string(),
                }],
                provenance: vec![CorrelationProvenanceDto {
                    source_kind: "timeline".to_string(),
                    source_record_id: "timeline-1".to_string(),
                    source_label: "FILE_MODIFIED".to_string(),
                    producer: Some("timeline.macb".to_string()),
                    producer_version: Some("1.0.0".to_string()),
                    guarantee_level: VerificationGuaranteeLevelDto::BestEffort,
                    warning_summary: Vec::new(),
                }],
                caveats: vec!["路径类匹配仍需回跳原始工件复核。".to_string()],
            }],
        };

        let json = serde_json::to_value(snapshot).unwrap();

        assert_eq!(json["generatedAt"], "2026-06-12T00:00:00Z");
        assert_eq!(json["nodeCount"], 3);
        assert_eq!(json["familyCoverage"][0]["family"], "LNK");
        assert_eq!(json["familyCoverage"][0]["status"], "covered");
        assert_eq!(json["nodes"][0]["sourceObjectId"], "file-1");
        assert_eq!(json["edges"][0]["kind"], "pathMatch");
        assert_eq!(json["edges"][0]["fromNodeId"], "artifact:1");
        assert_eq!(json["clusters"][0]["families"][0], "LNK");
        assert_eq!(json["clusters"][0]["primaryFileId"], "file-1");
        assert_eq!(json["leads"][0]["families"][0], "LNK");
        assert_eq!(
            json["leads"][0]["matchSignals"][0],
            "LNK 目标路径命中文件路径"
        );
        assert_eq!(json["leads"][0]["supportingNodeIds"][0], "artifact:1");
        assert_eq!(
            json["leads"][0]["provenance"][0]["guaranteeLevel"],
            "bestEffort"
        );
        assert!(json.get("generated_at").is_none());
        assert!(json.get("family_coverage").is_none());
    }
}
