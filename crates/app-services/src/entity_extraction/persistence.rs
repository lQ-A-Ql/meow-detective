use std::collections::HashSet;

use chrono::Utc;
use domain::{EdgeType, GraphEdge, GraphNode, NodeType};
use persistence_sqlite::repositories::{entity_repo, graph_repo::GraphRepo};
use rusqlite::Connection;

use super::scan::EntityMap;
use super::EntityExtractionError;

pub(super) fn persist_entity_graph(
    conn: &Connection,
    case_id: &str,
    entities: EntityMap,
) -> Result<u64, EntityExtractionError> {
    let now = Utc::now().to_rfc3339();
    entity_repo::delete_entity_nodes(conn, case_id).map_err(EntityExtractionError::Db)?;
    ensure_artifact_nodes(conn, case_id, &now)?;

    let (nodes, edges) = project_entities(case_id, &now, entities);
    let total = nodes.len() as u64;
    let graph_repo = GraphRepo::new(conn);
    if !nodes.is_empty() {
        graph_repo
            .insert_nodes_batch(&nodes)
            .map_err(|error| EntityExtractionError::Other(format!("graph node insert: {error}")))?;
    }
    if !edges.is_empty() {
        graph_repo
            .insert_edges_batch(&edges)
            .map_err(|error| EntityExtractionError::Other(format!("graph edge insert: {error}")))?;
    }
    Ok(total)
}

fn ensure_artifact_nodes(
    conn: &Connection,
    case_id: &str,
    now: &str,
) -> Result<(), EntityExtractionError> {
    let existing: HashSet<String> = entity_repo::get_existing_artifact_node_ids(conn, case_id)
        .map_err(EntityExtractionError::Db)?
        .into_iter()
        .collect();
    let mut rows = entity_repo::get_artifact_rows_for_case(conn, case_id)
        .map_err(EntityExtractionError::Db)?;
    rows.sort_by(|left, right| left.0.cmp(&right.0));

    let missing: Vec<GraphNode> = rows
        .into_iter()
        .filter(|(id, _, _, _)| !existing.contains(id))
        .map(|(id, title, summary, _)| GraphNode {
            id,
            case_id: case_id.to_string(),
            node_type: NodeType::Artifact,
            label: title,
            summary,
            tags: Vec::new(),
            created_at: now.to_string(),
        })
        .collect();
    if missing.is_empty() {
        return Ok(());
    }

    GraphRepo::new(conn)
        .insert_nodes_batch(&missing)
        .map_err(|error| {
            EntityExtractionError::Other(format!("graph node insert (artifact): {error}"))
        })
        .map(|_| ())
}

fn project_entities(
    case_id: &str,
    now: &str,
    entities: EntityMap,
) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let mut nodes = Vec::with_capacity(entities.len());
    let mut edges = Vec::new();
    for ((value, entity_type), source_ids) in entities {
        let node_id = format!("entity:{}:{}", case_id, uuid::Uuid::new_v4().as_simple());
        let (type_tag, summary) = entity_metadata(&entity_type);
        nodes.push(GraphNode {
            id: node_id.clone(),
            case_id: case_id.to_string(),
            node_type: NodeType::Entity,
            label: value,
            summary: summary.to_string(),
            tags: vec!["entity".into(), type_tag.into()],
            created_at: now.to_string(),
        });
        edges.extend(source_ids.into_iter().map(|artifact_id| GraphEdge {
            id: format!("derives_from:{node_id}:{artifact_id}"),
            case_id: case_id.to_string(),
            source_id: node_id.clone(),
            target_id: artifact_id,
            edge_type: EdgeType::DerivesFrom,
            confidence: None,
            provenance: Some("entity_extraction_v1".into()),
            created_at: now.to_string(),
        }));
    }
    edges.sort_by(|left, right| left.id.cmp(&right.id));
    (nodes, edges)
}

fn entity_metadata(entity_type: &str) -> (&'static str, &'static str) {
    match entity_type {
        "person" => ("person", "Email address"),
        "account" => ("account", "Windows security identifier (SID)"),
        "device" => ("device", "Hostname / computer name"),
        _ => ("entity", "Extracted entity"),
    }
}
