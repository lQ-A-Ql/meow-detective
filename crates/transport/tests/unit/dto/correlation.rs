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
