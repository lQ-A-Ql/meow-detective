use super::super::{CorrelationError, MAX_CORRELATION_ARTIFACTS, MAX_CORRELATION_TIMELINE_ROWS};
use super::cache::{
    collect_artifact_ids, compute_artifact_hash, get_cached_snapshot, resolve_case_id,
    store_cached_snapshot, CachedSnapshot,
};
use super::{
    append_rule_group, append_source_group, build_rule_groups, build_source_groups, empty_snapshot,
    finalize_snapshot_counts, merge_source_snapshot, persist_correlation_edges,
};
use chrono::Utc;
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use transport::dto::{
    ArtifactRowDto, CorrelationEdgeDto, CorrelationNodeDto, CorrelationSnapshotDto,
    TimelineEventDto,
};

pub fn get_correlation_snapshot(
    conn: &Connection,
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    let Some(case_id) = resolve_case_id(conn)? else {
        return compute_correlation_snapshot(conn);
    };
    let artifact_hash = compute_artifact_hash(conn)?;
    if let Some(cached) = get_cached_snapshot(conn, &case_id)? {
        if cached.artifact_hash == artifact_hash {
            return deserialize_cached(&cached);
        }
    }
    compute_and_store(conn, &case_id, &artifact_hash, collect_artifact_ids(conn)?)
}

pub fn get_correlation_snapshot_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    let mut merged = empty_snapshot();
    for (source_id, source_conn) in
        crate::source_db::open_ready_source_connections(case_conn, case_root, case_id)?
    {
        merge_source_snapshot(
            &mut merged,
            get_correlation_snapshot(&source_conn)?,
            &source_id,
        );
    }
    finalize_snapshot_counts(&mut merged);
    Ok(merged)
}

pub fn get_correlation_snapshot_incremental(
    conn: &Connection,
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    let Some(case_id) = resolve_case_id(conn)? else {
        return compute_correlation_snapshot(conn);
    };
    let artifact_hash = compute_artifact_hash(conn)?;
    let Some(cached) = get_cached_snapshot(conn, &case_id)? else {
        return compute_and_store(conn, &case_id, &artifact_hash, collect_artifact_ids(conn)?);
    };
    if cached.artifact_hash == artifact_hash {
        return deserialize_cached(&cached);
    }
    update_cached_snapshot(conn, &case_id, &artifact_hash, cached)
}

fn update_cached_snapshot(
    conn: &Connection,
    case_id: &str,
    artifact_hash: &str,
    cached: CachedSnapshot,
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    let current_ids = collect_artifact_ids(conn)?;
    let cached_ids: BTreeSet<String> =
        serde_json::from_str(&cached.artifact_ids_json).unwrap_or_default();
    let new_ids = current_ids
        .difference(&cached_ids)
        .cloned()
        .collect::<BTreeSet<_>>();
    if new_ids.is_empty() || new_ids.len() > current_ids.len() / 2 {
        return compute_and_store(conn, case_id, artifact_hash, current_ids);
    }
    let snapshot = compute_incremental_snapshot(conn, cached, &new_ids)?;
    store_snapshot(conn, case_id, &snapshot, artifact_hash, &current_ids)?;
    Ok(snapshot)
}

fn compute_and_store(
    conn: &Connection,
    case_id: &str,
    artifact_hash: &str,
    artifact_ids: BTreeSet<String>,
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    let snapshot = compute_correlation_snapshot(conn)?;
    store_snapshot(conn, case_id, &snapshot, artifact_hash, &artifact_ids)?;
    Ok(snapshot)
}

fn store_snapshot(
    conn: &Connection,
    case_id: &str,
    snapshot: &CorrelationSnapshotDto,
    artifact_hash: &str,
    artifact_ids: &BTreeSet<String>,
) -> Result<(), CorrelationError> {
    let ids_json = serde_json::to_string(artifact_ids)
        .map_err(|error| CorrelationError::Other(format!("serialize artifact ids: {error}")))?;
    store_cached_snapshot(conn, case_id, snapshot, artifact_hash, &ids_json)
}

fn deserialize_cached(cached: &CachedSnapshot) -> Result<CorrelationSnapshotDto, CorrelationError> {
    let mut snapshot = serde_json::from_str::<CorrelationSnapshotDto>(&cached.snapshot_json)
        .map_err(|error| {
            CorrelationError::Other(format!("deserialize cached snapshot: {error}"))
        })?;
    snapshot.generated_at = Utc::now().to_rfc3339();
    Ok(snapshot)
}

fn compute_correlation_snapshot(
    conn: &Connection,
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    let artifacts = load_artifacts(conn)?;
    let timelines = load_timelines(conn)?;
    build_snapshot_from(conn, &artifacts, &timelines)
}

fn load_artifacts(conn: &Connection) -> Result<Vec<ArtifactRowDto>, CorrelationError> {
    load_artifact_rows(conn).map(|rows| rows.into_iter().take(MAX_CORRELATION_ARTIFACTS).collect())
}

fn load_artifact_rows(conn: &Connection) -> Result<Vec<ArtifactRowDto>, CorrelationError> {
    crate::artifact_service::get_artifact_rows_from_db(conn, None)
        .map_err(|error| CorrelationError::Other(error.to_string()))
}

fn load_timelines(conn: &Connection) -> Result<Vec<TimelineEventDto>, CorrelationError> {
    crate::timeline_service::query_timeline(conn, 0, MAX_CORRELATION_TIMELINE_ROWS)
        .map_err(|error| CorrelationError::Other(error.to_string()))
        .map(|page| {
            page.items
                .into_iter()
                .filter(|row| !row.source_object_id.trim().is_empty())
                .collect()
        })
}

fn compute_incremental_snapshot(
    conn: &Connection,
    cached: CachedSnapshot,
    new_ids: &BTreeSet<String>,
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    let artifacts = load_artifact_rows(conn)?
        .into_iter()
        .filter(|artifact| new_ids.contains(&artifact.id))
        .take(MAX_CORRELATION_ARTIFACTS)
        .collect::<Vec<_>>();
    let timelines = load_timelines(conn)?;
    let source_groups = build_source_groups(conn, artifacts.clone(), timelines.clone())?;
    let rule_groups = build_rule_groups(conn, &artifacts, &timelines)?;
    let mut snapshot = deserialize_cached(&cached)?;
    append_groups(&mut snapshot, source_groups, rule_groups);
    super::ordering::finalize_local_snapshot(&mut snapshot);
    let _ = persist_correlation_edges(conn, &snapshot.leads);
    Ok(snapshot)
}

fn build_snapshot_from(
    conn: &Connection,
    artifacts: &[ArtifactRowDto],
    timelines: &[TimelineEventDto],
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    let source_groups = build_source_groups(conn, artifacts.to_vec(), timelines.to_vec())?;
    let rule_groups = build_rule_groups(conn, artifacts, timelines)?;
    let mut snapshot = empty_snapshot();
    append_groups(&mut snapshot, source_groups, rule_groups);
    super::ordering::finalize_local_snapshot(&mut snapshot);
    let _ = persist_correlation_edges(conn, &snapshot.leads);
    Ok(snapshot)
}

fn append_groups(
    snapshot: &mut CorrelationSnapshotDto,
    source_groups: Vec<super::super::CorrelationSourceGroup>,
    rule_groups: Vec<super::super::CorrelationRuleGroup>,
) {
    let mut node_map = take_node_map(snapshot);
    let mut edge_map = take_edge_map(snapshot);
    for group in source_groups {
        append_source_group(
            &group,
            &mut node_map,
            &mut edge_map,
            &mut snapshot.clusters,
            &mut snapshot.leads,
        );
    }
    for group in rule_groups {
        append_rule_group(
            &group,
            &mut node_map,
            &mut edge_map,
            &mut snapshot.clusters,
            &mut snapshot.leads,
        );
    }
    snapshot.nodes = node_map.into_values().collect();
    snapshot.edges = edge_map.into_values().collect();
}

fn take_node_map(snapshot: &mut CorrelationSnapshotDto) -> BTreeMap<String, CorrelationNodeDto> {
    snapshot
        .nodes
        .drain(..)
        .map(|node| (node.id.clone(), node))
        .collect()
}

fn take_edge_map(snapshot: &mut CorrelationSnapshotDto) -> BTreeMap<String, CorrelationEdgeDto> {
    snapshot
        .edges
        .drain(..)
        .map(|edge| (edge.id.clone(), edge))
        .collect()
}
