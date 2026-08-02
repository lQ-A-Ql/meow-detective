use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Mutex,
};

use chrono::Utc;
use domain::{EdgeType, GraphEdge, GraphNode, NodeType};
use once_cell::sync::Lazy;
use persistence_sqlite::repositories::{
    case_graph_repo::{CaseGraphProjection, CaseGraphRepo},
    graph_repo::GraphRepo,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::entity_resolution::EntityMergeEngine;

use super::{
    manifest::{collect_case_graph_manifest, CaseGraphManifest},
    CASE_GRAPH_PROJECTION_VERSION,
};
use crate::graph_service::GraphServiceError;

const CASE_GRAPH_DB_RELATIVE_PATH: &str = "indexes/case-graph.db";
const MAX_ENTITY_NODES: usize = 250_000;
const MAX_DEFAULT_SEEDS: usize = 12;
static CASE_GRAPH_BUILD_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub(crate) struct CaseGraphHandle {
    pub connection: Connection,
    pub projection: CaseGraphProjection,
}

struct EntityMember {
    data_source_id: String,
    node: GraphNode,
}

struct ProjectedOverlay {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    seed_ids: Vec<String>,
    hub_count: u64,
}

pub(crate) fn ensure_case_graph(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &str,
) -> Result<CaseGraphHandle, GraphServiceError> {
    let _guard = CASE_GRAPH_BUILD_LOCK
        .lock()
        .map_err(|_| GraphServiceError::Other("case graph build lock is poisoned".to_string()))?;
    let path = case_graph_path(case_root);
    for _ in 0..2 {
        let manifest = collect_case_graph_manifest(case_conn, case_root, case_id)?;
        let writer = persistence_sqlite::open_or_create_case_graph(&path)?;
        if let Some(projection) = current_projection(&writer, case_id, &manifest.digest)? {
            drop(writer);
            return open_handle(&path, projection);
        }

        let overlay = build_overlay(case_id, &manifest)?;
        let verified = collect_case_graph_manifest(case_conn, case_root, case_id)?;
        if verified.digest != manifest.digest {
            continue;
        }
        let projection = CaseGraphProjection {
            case_id: case_id.to_string(),
            projection_version: CASE_GRAPH_PROJECTION_VERSION.to_string(),
            source_manifest: manifest.digest,
            built_at: Utc::now().to_rfc3339(),
            source_count: manifest.sources.len() as u32,
            cross_source_entity_count: overlay.hub_count,
            cross_source_edge_count: overlay.edges.len() as u64,
            seed_ids: overlay.seed_ids,
        };
        let source_states = manifest
            .sources
            .iter()
            .map(|source| source.state.clone())
            .collect::<Vec<_>>();
        CaseGraphRepo::new(&writer).replace_projection(
            &projection,
            &source_states,
            &overlay.nodes,
            &overlay.edges,
        )?;
        drop(writer);
        return open_handle(&path, projection);
    }
    Err(GraphServiceError::Other(
        "source graph changed repeatedly while building the case graph; retry after analysis completes"
            .to_string(),
    ))
}

fn case_graph_path(case_root: &Path) -> PathBuf {
    case_root.join(CASE_GRAPH_DB_RELATIVE_PATH)
}

fn current_projection(
    connection: &Connection,
    case_id: &str,
    source_manifest: &str,
) -> Result<Option<CaseGraphProjection>, GraphServiceError> {
    Ok(CaseGraphRepo::new(connection)
        .get_projection()?
        .filter(|projection| {
            projection.case_id == case_id
                && projection.projection_version == CASE_GRAPH_PROJECTION_VERSION
                && projection.source_manifest == source_manifest
        }))
}

fn open_handle(
    path: &Path,
    projection: CaseGraphProjection,
) -> Result<CaseGraphHandle, GraphServiceError> {
    Ok(CaseGraphHandle {
        connection: persistence_sqlite::open_existing_case_graph_read_only(path)?,
        projection,
    })
}

fn build_overlay(
    case_id: &str,
    manifest: &CaseGraphManifest,
) -> Result<ProjectedOverlay, GraphServiceError> {
    let mut groups = BTreeMap::<(String, String), Vec<EntityMember>>::new();
    let mut entity_count = 0usize;
    for source in &manifest.sources {
        let connection = persistence_sqlite::open_existing_source_read_only(&source.database_path)?;
        let remaining = MAX_ENTITY_NODES.saturating_sub(entity_count);
        let nodes = GraphRepo::new(&connection).list_nodes_by_type_for_case_bounded(
            case_id,
            &NodeType::Entity,
            remaining.saturating_add(1) as u32,
        )?;
        if nodes.len() > remaining {
            return Err(entity_limit_error());
        }
        entity_count = entity_count.saturating_add(nodes.len());
        collect_entity_groups(&mut groups, &source.data_source_id, nodes);
    }
    project_groups(case_id, groups)
}

fn entity_limit_error() -> GraphServiceError {
    GraphServiceError::Unsupported(format!(
        "case graph entity projection exceeds the supported limit of {MAX_ENTITY_NODES} nodes"
    ))
}

fn collect_entity_groups(
    groups: &mut BTreeMap<(String, String), Vec<EntityMember>>,
    data_source_id: &domain::DataSourceId,
    nodes: Vec<GraphNode>,
) {
    for node in nodes {
        let entity_type = EntityMergeEngine::entity_type_from_tags(&node.tags);
        if matches!(entity_type.as_str(), "unknown" | "entity") {
            continue;
        }
        let canonical = EntityMergeEngine::canonicalize_entity(&node.label, &entity_type);
        if canonical.is_empty() {
            continue;
        }
        groups
            .entry((entity_type, canonical))
            .or_default()
            .push(EntityMember {
                data_source_id: data_source_id.0.clone(),
                node,
            });
    }
}

fn project_groups(
    case_id: &str,
    groups: BTreeMap<(String, String), Vec<EntityMember>>,
) -> Result<ProjectedOverlay, GraphServiceError> {
    let built_at = Utc::now().to_rfc3339();
    let mut mirrored = BTreeMap::<String, GraphNode>::new();
    let mut hubs = Vec::<(usize, usize, GraphNode)>::new();
    let mut edges = Vec::new();
    for ((entity_type, canonical), mut members) in groups {
        members.sort_by(|left, right| {
            left.data_source_id
                .cmp(&right.data_source_id)
                .then_with(|| left.node.id.cmp(&right.node.id))
        });
        let source_ids = members
            .iter()
            .map(|member| member.data_source_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if source_ids.len() < 2 {
            continue;
        }
        let member_count = members.len();
        let hub_id = case_entity_id(&entity_type, &canonical);
        let label = members
            .first()
            .map(|member| member.node.label.clone())
            .unwrap_or_else(|| canonical.clone());
        let hub = GraphNode {
            id: hub_id.clone(),
            case_id: case_id.to_string(),
            node_type: NodeType::Entity,
            label,
            summary: format!(
                "Exact {entity_type} identity shared by {} data sources",
                source_ids.len()
            ),
            tags: vec![
                "entity".to_string(),
                entity_type.clone(),
                "case-entity".to_string(),
                "cross-source".to_string(),
            ],
            created_at: built_at.clone(),
        };
        for member in members {
            let source_id = domain::DataSourceId(member.data_source_id.clone());
            let scoped = scope_node(member.node, &source_id);
            let edge_id = case_edge_id(&hub_id, &scoped.id);
            let provenance = serde_json::json!({
                "kind": "case_cross_source_entity",
                "lead_id": "case-cross-source-entity-exact-v1",
                "families": ["entity_resolution"],
                "match_signals": ["exact canonical entity identity across data sources"],
                "entity_type": entity_type,
                "canonical_hash": identity_hash(&entity_type, &canonical),
                "data_source_id": member.data_source_id,
                "peer_data_source_ids": source_ids,
                "projection_version": CASE_GRAPH_PROJECTION_VERSION,
            })
            .to_string();
            edges.push(GraphEdge {
                id: edge_id,
                case_id: case_id.to_string(),
                source_id: scoped.id.clone(),
                target_id: hub_id.clone(),
                edge_type: EdgeType::CorrelatesWith,
                confidence: Some(1.0),
                provenance: Some(provenance),
                created_at: built_at.clone(),
            });
            mirrored.insert(scoped.id.clone(), scoped);
        }
        hubs.push((source_ids.len(), member_count, hub));
    }
    hubs.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.2.id.cmp(&right.2.id))
    });
    let seed_ids = hubs
        .iter()
        .take(MAX_DEFAULT_SEEDS)
        .map(|(_, _, node)| node.id.clone())
        .collect();
    let hub_count = hubs.len() as u64;
    let mut nodes = mirrored.into_values().collect::<Vec<_>>();
    nodes.extend(hubs.into_iter().map(|(_, _, node)| node));
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    edges.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(ProjectedOverlay {
        nodes,
        edges,
        seed_ids,
        hub_count,
    })
}

fn case_entity_id(entity_type: &str, canonical: &str) -> String {
    format!("case:entity:{}", identity_hash(entity_type, canonical))
}

fn case_edge_id(hub_id: &str, source_id: &str) -> String {
    let digest = Sha256::digest(format!("{hub_id}\0{source_id}").as_bytes());
    format!("case:edge:correlates:{}", hex::encode(digest))
}

fn identity_hash(entity_type: &str, canonical: &str) -> String {
    hex::encode(Sha256::digest(
        format!("{entity_type}\0{canonical}").as_bytes(),
    ))
}

fn scope_node(mut node: GraphNode, source_id: &domain::DataSourceId) -> GraphNode {
    node.id = crate::source_db::encode_source_scoped_id(source_id, &node.id);
    node
}
