use super::super::{CorrelationError, MAX_CORRELATION_ARTIFACTS, MAX_CORRELATION_TIMELINE_ROWS};
use super::cache::{
    artifact_generation_key, get_cached_snapshot, resolve_case_id, store_cached_snapshot,
    CachedSnapshot,
};
use super::inputs::{append_only, input_key, matches_revision};
use super::{
    append_rule_group, append_source_group, build_rule_groups, build_source_groups, empty_snapshot,
    finalize_snapshot_counts, merge_source_snapshot, persist_correlation_edges,
};
use crate::artifact_service::artifact_to_dto;
use chrono::Utc;
use persistence_source::repositories::artifact_repo::ArtifactRepo;
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use transport::dto::{
    ArtifactRowDto, CorrelationEdgeDto, CorrelationNodeDto, CorrelationSnapshotDto,
    TimelineEventDto,
};

pub fn get_correlation_snapshot(
    conn: &Connection,
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    if persistence_source::detail::source_id(conn)?.is_none() {
        return compute_correlation_snapshot(conn);
    }
    let Some(case_id) = resolve_case_id(conn)? else {
        return compute_correlation_snapshot(conn);
    };
    let artifact_hash = artifact_generation_key(conn)?;
    if let Some(cached) = get_cached_snapshot(conn, &case_id)? {
        if matches_revision(&cached, &artifact_hash) {
            return deserialize_cached(&cached);
        }
    }
    compute_and_store(conn, &case_id, &artifact_hash, load_artifacts(conn)?)
}

pub fn get_correlation_snapshot_read_only(
    conn: &Connection,
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    if persistence_source::detail::source_id(conn)?.is_none() {
        return compute_correlation_snapshot_read_only(conn);
    }
    if let Some(case_id) = resolve_case_id(conn)? {
        let artifact_hash = artifact_generation_key(conn)?;
        if let Some(cached) = get_cached_snapshot(conn, &case_id)? {
            if matches_revision(&cached, &artifact_hash) {
                return deserialize_cached(&cached);
            }
        }
    }
    compute_correlation_snapshot_read_only(conn)
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

pub fn get_correlation_snapshot_read_only_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    let mut merged = empty_snapshot();
    for (source_id, source_conn) in
        crate::source_db::open_ready_source_connections_read_only(case_conn, case_root, case_id)?
    {
        merge_source_snapshot(
            &mut merged,
            get_correlation_snapshot_read_only(&source_conn)?,
            &source_id,
        );
    }
    finalize_snapshot_counts(&mut merged);
    Ok(merged)
}

pub fn rebuild_correlation_snapshot_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    cancel_token: &AtomicBool,
) -> Result<(), CorrelationError> {
    ensure_not_cancelled(cancel_token)?;
    get_correlation_snapshot_for_case(case_conn, case_root, case_id)?;
    ensure_not_cancelled(cancel_token)
}

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> Result<(), CorrelationError> {
    if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
        Err(CorrelationError::Cancelled)
    } else {
        Ok(())
    }
}

pub fn get_correlation_snapshot_incremental(
    conn: &Connection,
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    if persistence_source::detail::source_id(conn)?.is_none() {
        return compute_correlation_snapshot(conn);
    }
    let Some(case_id) = resolve_case_id(conn)? else {
        return compute_correlation_snapshot(conn);
    };
    let artifact_hash = artifact_generation_key(conn)?;
    let Some(cached) = get_cached_snapshot(conn, &case_id)? else {
        return compute_and_store(conn, &case_id, &artifact_hash, load_artifacts(conn)?);
    };
    if matches_revision(&cached, &artifact_hash) {
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
    let artifacts = load_artifacts(conn)?;
    let current_ids = artifacts
        .iter()
        .map(|artifact| artifact.id.clone())
        .collect::<BTreeSet<_>>();
    let cached_ids: BTreeSet<String> = serde_json::from_str(&cached.artifact_ids_json)
        .map_err(|error| CorrelationError::Other(error.to_string()))?;
    let new_ids = current_ids
        .difference(&cached_ids)
        .cloned()
        .collect::<BTreeSet<_>>();
    if !append_only(&cached, artifact_hash, &artifacts)? {
        return compute_and_store(conn, case_id, artifact_hash, artifacts);
    }
    let snapshot = compute_incremental_snapshot(conn, cached, &new_ids, &artifacts)?;
    store_snapshot(conn, case_id, &snapshot, artifact_hash, &artifacts)?;
    Ok(snapshot)
}

fn compute_and_store(
    conn: &Connection,
    case_id: &str,
    artifact_hash: &str,
    artifacts: Vec<ArtifactRowDto>,
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    let snapshot = build_snapshot_from(conn, &artifacts, &load_timelines(conn)?)?;
    store_snapshot(conn, case_id, &snapshot, artifact_hash, &artifacts)?;
    Ok(snapshot)
}

fn store_snapshot(
    conn: &Connection,
    case_id: &str,
    snapshot: &CorrelationSnapshotDto,
    artifact_hash: &str,
    artifacts: &[ArtifactRowDto],
) -> Result<(), CorrelationError> {
    let artifact_ids = artifacts
        .iter()
        .map(|artifact| &artifact.id)
        .collect::<BTreeSet<_>>();
    let ids_json = serde_json::to_string(&artifact_ids)
        .map_err(|error| CorrelationError::Other(format!("serialize artifact ids: {error}")))?;
    store_cached_snapshot(
        conn,
        case_id,
        snapshot,
        &input_key(artifact_hash, artifacts)?,
        &ids_json,
    )
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

fn compute_correlation_snapshot_read_only(
    conn: &Connection,
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    let artifacts = load_artifacts(conn)?;
    let timelines = load_timelines(conn)?;
    build_snapshot_from_read_only(conn, &artifacts, &timelines)
}

/// Load at most [`MAX_CORRELATION_ARTIFACTS`] artifacts straight from the
/// created-order index (`created_at DESC, id ASC`), newest first, instead of
/// materializing every artifact DTO and truncating afterwards.
fn load_artifacts(conn: &Connection) -> Result<Vec<ArtifactRowDto>, CorrelationError> {
    if persistence_source::detail::source_id(conn)?.is_none() {
        return Ok(Vec::new());
    }
    let rows = ArtifactRepo::new(conn)
        .list_by_family_after(None, None, MAX_CORRELATION_ARTIFACTS as u32)
        .map_err(|error| CorrelationError::Other(error.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|row| artifact_to_dto(&row.artifact))
        .collect())
}

fn load_timelines(conn: &Connection) -> Result<Vec<TimelineEventDto>, CorrelationError> {
    if persistence_source::detail::source_id(conn)?.is_none() {
        return Ok(Vec::new());
    }
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
    current: &[ArtifactRowDto],
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    let artifacts = current
        .iter()
        .filter(|artifact| new_ids.contains(&artifact.id))
        .cloned()
        .collect::<Vec<_>>();
    let timelines = load_timelines(conn)?;
    let mut added = build_snapshot_from(conn, &artifacts, &timelines)?;
    let mut snapshot = deserialize_cached(&cached)?;
    let existing = snapshot
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    if added
        .nodes
        .iter()
        .any(|node| existing.contains(node.id.as_str()))
    {
        return build_snapshot_from(conn, current, &timelines);
    }
    snapshot.nodes.append(&mut added.nodes);
    snapshot.edges.append(&mut added.edges);
    snapshot.clusters.append(&mut added.clusters);
    snapshot.leads.append(&mut added.leads);
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

fn build_snapshot_from_read_only(
    conn: &Connection,
    artifacts: &[ArtifactRowDto],
    timelines: &[TimelineEventDto],
) -> Result<CorrelationSnapshotDto, CorrelationError> {
    let source_groups = build_source_groups(conn, artifacts.to_vec(), timelines.to_vec())?;
    let rule_groups = build_rule_groups(conn, artifacts, timelines)?;
    let mut snapshot = empty_snapshot();
    append_groups(&mut snapshot, source_groups, rule_groups);
    super::ordering::finalize_local_snapshot(&mut snapshot);
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
