use std::collections::HashSet;

use chrono::Utc;
use domain::{GraphEdge, GraphNode};
use persistence_sqlite::repositories::graph_repo::GraphRepo;
use rusqlite::Connection;

use super::super::error::RulePackError;
use super::super::parser::{RuleDefinition, RulePack};
use super::loading::ArtifactRow;

pub(super) fn ensure_artifact_nodes(
    conn: &Connection,
    case_id: &str,
    family: &str,
    artifacts: &[ArtifactRow],
) {
    let now = Utc::now().to_rfc3339();
    let nodes: Vec<GraphNode> = artifacts
        .iter()
        .map(|artifact| GraphNode {
            id: artifact.id.clone(),
            case_id: case_id.to_string(),
            node_type: domain::NodeType::Artifact,
            label: format!("{family} artifact"),
            summary: format!("{family} rule pack artifact"),
            tags: Vec::new(),
            created_at: now.clone(),
        })
        .collect();
    let _ = GraphRepo::new(conn).insert_nodes_batch(&nodes);
}

pub(super) fn project_edge(
    case_id: &str,
    pack: &RulePack,
    rule: &RuleDefinition,
    source_id: &str,
    target_id: &str,
    created_at: &str,
) -> GraphEdge {
    GraphEdge {
        id: format!(
            "rp:{}:{}:{}:{}",
            pack.manifest.name, rule.id, source_id, target_id
        ),
        case_id: case_id.to_string(),
        source_id: source_id.to_string(),
        target_id: target_id.to_string(),
        edge_type: rule.edge_type.clone().into(),
        confidence: Some(confidence_number(&rule.match_signals.confidence)),
        provenance: Some(provenance(pack, rule)),
        created_at: created_at.to_string(),
    }
}

pub(super) fn executed_rule_ids(
    conn: &Connection,
    case_id: &str,
    pack_id: &str,
) -> Result<HashSet<String>, RulePackError> {
    let rows = GraphRepo::new(conn)
        .find_edges_with_provenance_by_case(case_id, "correlates_with")
        .map_err(|error| RulePackError::Other(format!("query executed rule ids: {error}")))?;
    Ok(rows
        .into_iter()
        .filter_map(|row| serde_json::from_str::<serde_json::Value>(&row).ok())
        .filter(|value| value.get("pack_id").and_then(|item| item.as_str()) == Some(pack_id))
        .filter_map(|value| {
            value
                .get("rule_id")
                .and_then(|item| item.as_str())
                .map(str::to_string)
        })
        .collect())
}

fn provenance(pack: &RulePack, rule: &RuleDefinition) -> String {
    serde_json::json!({
        "pack_id": pack.manifest.name,
        "rule_id": rule.id,
        "pack_version": pack.manifest.version,
        "kind": "rule_pack",
    })
    .to_string()
}

fn confidence_number(confidence: &str) -> f64 {
    match confidence {
        "direct" => 1.0,
        "strong" => 0.9,
        "weak" => 0.5,
        "heuristic" => 0.3,
        _ => 0.5,
    }
}
