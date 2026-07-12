use chrono::Utc;
use domain::{EdgeType, FileEntryId, GraphEdge, GraphNode, NodeType};
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, graph_repo::GraphRepo};
use rusqlite::Connection;

use super::ArtifactServiceError;

pub fn store_artifacts(
    conn: &Connection,
    artifacts: &[domain::Artifact],
    case_id: &str,
    data_source_id: &str,
) -> Result<(), ArtifactServiceError> {
    if artifacts.is_empty() {
        return Ok(());
    }
    ArtifactRepo::new(conn).insert_batch(artifacts, case_id, data_source_id)?;

    // Graph projection is deliberately best-effort and never blocks artifact persistence.
    if let Err(error) = populate_artifact_graph(conn, artifacts, case_id) {
        tracing::warn!("artifact graph population failed (non-fatal): {error}");
    }
    Ok(())
}

pub(super) fn already_has_artifact_for_source(
    conn: &Connection,
    source_object_id: &str,
) -> Result<bool, ArtifactServiceError> {
    let count = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE source_object_id = ?1",
            [source_object_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| ArtifactServiceError::Db(persistence_sqlite::DbError::from(error)))?;
    Ok(count > 0)
}

fn populate_artifact_graph(
    conn: &Connection,
    artifacts: &[domain::Artifact],
    case_id: &str,
) -> Result<(), ArtifactServiceError> {
    let graph_repo = GraphRepo::new(conn);
    let created_at = Utc::now().to_rfc3339();
    let nodes = artifacts
        .iter()
        .map(|artifact| artifact_node(artifact, case_id, &created_at))
        .collect::<Vec<_>>();
    let edges = artifacts
        .iter()
        .filter_map(|artifact| artifact_edge(artifact, case_id, &created_at))
        .collect::<Vec<_>>();

    graph_repo
        .insert_nodes_batch(&nodes)
        .map_err(|error| ArtifactServiceError::other(format!("graph node insert: {error}")))?;
    if let Err(error) = graph_repo.insert_edges_batch(&edges) {
        tracing::warn!("artifact graph edge insert failed (non-fatal): {error}");
    }
    Ok(())
}

fn artifact_node(artifact: &domain::Artifact, case_id: &str, created_at: &str) -> GraphNode {
    GraphNode {
        id: artifact.id.0.clone(),
        case_id: case_id.to_string(),
        node_type: NodeType::Artifact,
        label: artifact.title.clone(),
        summary: artifact.family.clone(),
        tags: Vec::new(),
        created_at: created_at.to_string(),
    }
}

fn artifact_edge(
    artifact: &domain::Artifact,
    case_id: &str,
    created_at: &str,
) -> Option<GraphEdge> {
    let source_id: &FileEntryId = artifact.source_object_id.as_ref()?;
    Some(GraphEdge {
        id: format!("references:{}:{}", artifact.id.0, source_id.0),
        case_id: case_id.to_string(),
        source_id: artifact.id.0.clone(),
        target_id: source_id.0.clone(),
        edge_type: EdgeType::References,
        confidence: artifact.confidence.map(f64::from),
        provenance: artifact.extractor_id.clone(),
        created_at: created_at.to_string(),
    })
}
