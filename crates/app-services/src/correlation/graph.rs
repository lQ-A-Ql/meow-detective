use rayon::prelude::*;
use std::sync::Mutex;

use super::rules::{
    build_artifact_rule_matches, rule_match_paths, rule_match_text_needles, rule_match_timestamps,
    timeline_path_candidates, timeline_text_candidates,
};
use super::{
    artifact_family, confidence_rank, dedup_vec, edge_kind_token, has_family, insert_node,
    parse_rfc3339_utc, path_suffix_key, CorrelationRuleGroup, CorrelationRuleMatch,
    CorrelationSourceGroup, CORRELATION_RULE_FAMILIES, MAX_CORRELATION_ARTIFACTS,
    MAX_CORRELATION_TIMELINE_ROWS, RULE_TIMELINE_CONTEXT_LIMIT,
    RULE_TIMELINE_PROXIMITY_WINDOW_SECS,
};
use chrono::Utc;
use domain::{EdgeType, FileEntry, FileEntryId, GraphEdge};
use persistence_sqlite::repositories::{file_repo::FileRepo, graph_repo::GraphRepo};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use transport::dto::{
    ArtifactRowDto, CorrelationClusterDto, CorrelationConfidenceDto, CorrelationCoverageStatusDto,
    CorrelationEdgeDto, CorrelationEdgeKindDto, CorrelationFamilyCoverageDto,
    CorrelationJumpTargetDto, CorrelationLeadDto, CorrelationNodeDto, CorrelationNodeKindDto,
    CorrelationProvenanceDto, CorrelationSnapshotDto, TimelineEventDto,
    VerificationGuaranteeLevelDto,
};

// ── Main entry point ──

/// Returns a correlation snapshot with caching. On first call (or after artifact changes),
/// the full correlation pipeline runs; subsequent calls with the same artifact set return the
/// cached snapshot immediately.
pub fn get_correlation_snapshot(conn: &Connection) -> Result<CorrelationSnapshotDto, String> {
    let Some(case_id) = resolve_case_id(conn)? else {
        // No artifacts in the database yet — nothing to cache, but still compute
        // family coverage from the rule families constant.
        return compute_correlation_snapshot(conn);
    };
    let artifact_hash = compute_artifact_hash(conn)?;

    // Check cache first
    if let Some(cached) = get_cached_snapshot(conn, &case_id)? {
        if cached.artifact_hash == artifact_hash {
            let mut snapshot: CorrelationSnapshotDto = serde_json::from_str(&cached.snapshot_json)
                .map_err(|e| format!("deserialize cached snapshot: {e}"))?;
            // Refresh generated_at to reflect this read
            snapshot.generated_at = Utc::now().to_rfc3339();
            return Ok(snapshot);
        }
    }

    // Cache miss or hash changed — full recompute
    let snapshot = compute_correlation_snapshot(conn)?;
    let ids_json = serde_json::to_string(&collect_artifact_ids(conn)?)
        .map_err(|e| format!("serialize artifact ids: {e}"))?;
    store_cached_snapshot(conn, &case_id, &snapshot, &artifact_hash, &ids_json)?;
    Ok(snapshot)
}

/// Full uncached correlation computation (shared by get_correlation_snapshot and the incremental
/// path when incremental is not applicable).
fn compute_correlation_snapshot(conn: &Connection) -> Result<CorrelationSnapshotDto, String> {
    let artifacts = crate::artifact_service::get_artifact_rows_from_db(conn, None)?
        .into_iter()
        .take(MAX_CORRELATION_ARTIFACTS)
        .collect::<Vec<_>>();
    let timelines =
        crate::timeline_service::query_timeline(conn, 0, MAX_CORRELATION_TIMELINE_ROWS)?
            .items
            .into_iter()
            .filter(|row| !row.source_object_id.trim().is_empty())
            .collect::<Vec<_>>();

    build_snapshot_from(conn, &artifacts, &timelines)
}

/// Incremental correlation snapshot: only processes artifacts that are new since the last
/// cached snapshot. Falls back to full recompute when the cache is empty or more than half
/// the artifacts are new.
pub fn get_correlation_snapshot_incremental(
    conn: &Connection,
) -> Result<CorrelationSnapshotDto, String> {
    let Some(case_id) = resolve_case_id(conn)? else {
        // No artifacts — nothing to cache or diff, but still compute family coverage.
        return compute_correlation_snapshot(conn);
    };
    let artifact_hash = compute_artifact_hash(conn)?;

    let cached = match get_cached_snapshot(conn, &case_id)? {
        Some(c) => c,
        None => {
            // No cache — full compute
            let snapshot = compute_correlation_snapshot(conn)?;
            let ids_json = serde_json::to_string(&collect_artifact_ids(conn)?)
                .map_err(|e| format!("serialize artifact ids: {e}"))?;
            store_cached_snapshot(conn, &case_id, &snapshot, &artifact_hash, &ids_json)?;
            return Ok(snapshot);
        }
    };

    // Hash unchanged — return cached
    if cached.artifact_hash == artifact_hash {
        let mut snapshot: CorrelationSnapshotDto = serde_json::from_str(&cached.snapshot_json)
            .map_err(|e| format!("deserialize cached snapshot: {e}"))?;
        snapshot.generated_at = Utc::now().to_rfc3339();
        return Ok(snapshot);
    }

    // Find new artifact IDs
    let current_ids: BTreeSet<String> = collect_artifact_ids(conn)?;
    let cached_ids: BTreeSet<String> =
        serde_json::from_str(&cached.artifact_ids_json).unwrap_or_default();
    let new_ids: Vec<_> = current_ids.difference(&cached_ids).cloned().collect();

    // If too many new (or cached is stale), full recompute
    if new_ids.is_empty() || new_ids.len() > current_ids.len() / 2 {
        let snapshot = compute_correlation_snapshot(conn)?;
        let ids_json = serde_json::to_string(&current_ids)
            .map_err(|e| format!("serialize artifact ids: {e}"))?;
        store_cached_snapshot(conn, &case_id, &snapshot, &artifact_hash, &ids_json)?;
        return Ok(snapshot);
    }

    // ── Incremental path: only process new artifacts ──
    let new_artifacts: Vec<ArtifactRowDto> =
        crate::artifact_service::get_artifact_rows_from_db(conn, None)?
            .into_iter()
            .filter(|a| new_ids.contains(&a.id))
            .take(MAX_CORRELATION_ARTIFACTS)
            .collect();

    let timelines =
        crate::timeline_service::query_timeline(conn, 0, MAX_CORRELATION_TIMELINE_ROWS)?
            .items
            .into_iter()
            .filter(|row| !row.source_object_id.trim().is_empty())
            .collect::<Vec<_>>();

    // Build source groups and rule groups from new artifacts only.
    // Order matters: build_rule_groups borrows, build_source_groups takes ownership.
    let new_rule_groups = build_rule_groups(conn, &new_artifacts, &timelines)?;
    let new_source_groups = build_source_groups(conn, new_artifacts, timelines.clone())?;

    // Deserialize the cached snapshot and build its node/edge maps
    let mut cached: CorrelationSnapshotDto = serde_json::from_str(&cached.snapshot_json)
        .map_err(|e| format!("deserialize cached snapshot: {e}"))?;

    let mut node_map = cached
        .nodes
        .drain(..)
        .map(|n| (n.id.clone(), n))
        .collect::<BTreeMap<_, _>>();
    let mut edge_map = cached
        .edges
        .drain(..)
        .map(|e| (e.id.clone(), e))
        .collect::<BTreeMap<_, _>>();
    let mut clusters = cached.clusters;
    let mut leads = cached.leads;

    // Append new groups — append_* already deduplicates via insert_node
    for group in new_source_groups {
        append_source_group(
            &group,
            &mut node_map,
            &mut edge_map,
            &mut clusters,
            &mut leads,
        );
    }
    for group in new_rule_groups {
        append_rule_group(
            &group,
            &mut node_map,
            &mut edge_map,
            &mut clusters,
            &mut leads,
        );
    }

    let mut nodes = node_map.into_values().collect::<Vec<_>>();
    nodes.sort_by_key(|node| (node.kind.clone(), node.title.clone(), node.id.clone()));

    let mut edges = edge_map.into_values().collect::<Vec<_>>();
    edges.sort_by_key(|edge| (Reverse(confidence_rank(&edge.confidence)), edge.id.clone()));

    leads.sort_by_key(|lead| {
        (
            Reverse(confidence_rank(&lead.confidence)),
            Reverse(lead.supporting_node_ids.len()),
            lead.title.clone(),
        )
    });
    clusters.sort_by_key(|cluster| {
        (
            Reverse(confidence_rank(&cluster.confidence)),
            Reverse(cluster.node_ids.len()),
            cluster.title.clone(),
        )
    });
    let family_coverage = build_family_coverage(&leads, &clusters);

    let _ = persist_correlation_edges(conn, &leads);

    let snapshot = CorrelationSnapshotDto {
        generated_at: Utc::now().to_rfc3339(),
        node_count: nodes.len() as u32,
        edge_count: edges.len() as u32,
        cluster_count: clusters.len() as u32,
        lead_count: leads.len() as u32,
        family_coverage,
        nodes,
        edges,
        clusters,
        leads,
    };

    store_cached_snapshot(
        conn,
        &case_id,
        &snapshot,
        &artifact_hash,
        &serde_json::to_string(&current_ids).map_err(|e| format!("serialize artifact ids: {e}"))?,
    )?;
    Ok(snapshot)
}

// ── Cache helpers ──

struct CachedSnapshot {
    snapshot_json: String,
    artifact_hash: String,
    artifact_ids_json: String,
}

fn get_cached_snapshot(conn: &Connection, case_id: &str) -> Result<Option<CachedSnapshot>, String> {
    conn.query_row(
        "SELECT snapshot_json, artifact_hash, artifact_ids_json
         FROM correlation_snapshots WHERE case_id = ?1",
        params![case_id],
        |row| {
            Ok(CachedSnapshot {
                snapshot_json: row.get(0)?,
                artifact_hash: row.get(1)?,
                artifact_ids_json: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn store_cached_snapshot(
    conn: &Connection,
    case_id: &str,
    snapshot: &CorrelationSnapshotDto,
    artifact_hash: &str,
    artifact_ids_json: &str,
) -> Result<(), String> {
    let json = serde_json::to_string(snapshot)
        .map_err(|e| format!("serialize snapshot for cache: {e}"))?;
    conn.execute(
        "INSERT OR REPLACE INTO correlation_snapshots
         (case_id, snapshot_json, generated_at, artifact_hash, artifact_ids_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            case_id,
            json,
            snapshot.generated_at,
            artifact_hash,
            artifact_ids_json,
        ],
    )
    .map_err(|e| format!("store cached snapshot: {e}"))?;
    Ok(())
}

/// Invalidate the correlation cache for a given case (call after a new data source import).
pub fn invalidate_correlation_cache(conn: &Connection, case_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM correlation_snapshots WHERE case_id = ?1",
        params![case_id],
    )
    .map_err(|e| format!("invalidate correlation snapshots cache: {e}"))?;
    conn.execute(
        "DELETE FROM correlation_edges_cache WHERE case_id = ?1",
        params![case_id],
    )
    .map_err(|e| format!("invalidate correlation edges cache: {e}"))?;
    Ok(())
}

/// Compute a SHA-256 artifact hash over (sorted artifact id, created_at) pairs.
fn compute_artifact_hash(conn: &Connection) -> Result<String, String> {
    let mut stmt = conn
        .prepare("SELECT id, created_at FROM artifacts ORDER BY id")
        .map_err(|e| format!("compute artifact hash: {e}"))?;
    let mut hasher = Sha256::new();
    let rows = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let created_at: String = row.get(1)?;
            Ok((id, created_at))
        })
        .map_err(|e| format!("compute artifact hash query: {e}"))?;
    for row in rows {
        let (id, created_at) = row.map_err(|e| format!("compute artifact hash row: {e}"))?;
        hasher.update(id.as_bytes());
        hasher.update(b"|");
        hasher.update(created_at.as_bytes());
        hasher.update(b"\n");
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Collect all artifact IDs for cache tracking.
fn collect_artifact_ids(conn: &Connection) -> Result<BTreeSet<String>, String> {
    let mut stmt = conn
        .prepare("SELECT id FROM artifacts ORDER BY id")
        .map_err(|e| format!("collect artifact ids: {e}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("collect artifact ids query: {e}"))?;
    let mut ids = BTreeSet::new();
    for row in rows {
        ids.insert(row.map_err(|e| format!("collect artifact ids row: {e}"))?);
    }
    Ok(ids)
}

/// Resolve the case_id from the database (needed for cache key).
/// Returns `None` when the artifacts table is empty (nothing to correlate).
fn resolve_case_id(conn: &Connection) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT DISTINCT case_id FROM artifacts LIMIT 1",
        [],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|e| format!("resolve case_id: {e}"))
}

/// Build a CorrelationSnapshotDto from pre-fetched artifacts and timelines.
fn build_snapshot_from(
    conn: &Connection,
    artifacts: &[ArtifactRowDto],
    timelines: &[TimelineEventDto],
) -> Result<CorrelationSnapshotDto, String> {
    let groups = build_source_groups(conn, artifacts.to_vec(), timelines.to_vec())?;
    let rule_groups = build_rule_groups(conn, artifacts, timelines)?;

    let mut node_map = BTreeMap::<String, CorrelationNodeDto>::new();
    let mut edge_map = BTreeMap::<String, CorrelationEdgeDto>::new();
    let mut clusters = Vec::new();
    let mut leads = Vec::new();

    for group in groups {
        append_source_group(
            &group,
            &mut node_map,
            &mut edge_map,
            &mut clusters,
            &mut leads,
        );
    }

    for group in rule_groups {
        append_rule_group(
            &group,
            &mut node_map,
            &mut edge_map,
            &mut clusters,
            &mut leads,
        );
    }

    let mut nodes = node_map.into_values().collect::<Vec<_>>();
    nodes.sort_by_key(|node| (node.kind.clone(), node.title.clone(), node.id.clone()));

    let mut edges = edge_map.into_values().collect::<Vec<_>>();
    edges.sort_by_key(|edge| (Reverse(confidence_rank(&edge.confidence)), edge.id.clone()));

    leads.sort_by_key(|lead| {
        (
            Reverse(confidence_rank(&lead.confidence)),
            Reverse(lead.supporting_node_ids.len()),
            lead.title.clone(),
        )
    });
    clusters.sort_by_key(|cluster| {
        (
            Reverse(confidence_rank(&cluster.confidence)),
            Reverse(cluster.node_ids.len()),
            cluster.title.clone(),
        )
    });
    let family_coverage = build_family_coverage(&leads, &clusters);

    // Persist CorrelatesWith edges into the investigative graph
    let _ = persist_correlation_edges(conn, &leads);

    Ok(CorrelationSnapshotDto {
        generated_at: Utc::now().to_rfc3339(),
        node_count: nodes.len() as u32,
        edge_count: edges.len() as u32,
        cluster_count: clusters.len() as u32,
        lead_count: leads.len() as u32,
        family_coverage,
        nodes,
        edges,
        clusters,
        leads,
    })
}

// ── Source / rule group builders ──

pub(crate) fn build_source_groups(
    conn: &Connection,
    artifacts: Vec<ArtifactRowDto>,
    timelines: Vec<TimelineEventDto>,
) -> Result<Vec<CorrelationSourceGroup>, String> {
    let groups_map = Mutex::new(BTreeMap::<String, CorrelationSourceGroup>::new());

    artifacts.par_iter().for_each(|artifact| {
        let Some(ref source_object_id) = artifact.source_object_id else {
            return;
        };
        let mut groups = groups_map.lock().unwrap();
        let group =
            groups
                .entry(source_object_id.clone())
                .or_insert_with(|| CorrelationSourceGroup {
                    source_object_id: source_object_id.clone(),
                    ..CorrelationSourceGroup::default()
                });
        group.artifacts.push(artifact.clone());
    });

    timelines.par_iter().for_each(|timeline| {
        let mut groups = groups_map.lock().unwrap();
        let group = groups
            .entry(timeline.source_object_id.clone())
            .or_insert_with(|| CorrelationSourceGroup {
                source_object_id: timeline.source_object_id.clone(),
                ..CorrelationSourceGroup::default()
            });
        group.timelines.push(timeline.clone());
    });

    let groups = groups_map.into_inner().unwrap();

    let repo = FileRepo::new(conn);
    let mut items = groups.into_values().collect::<Vec<_>>();
    for group in &mut items {
        group.file = repo
            .find_by_id(&FileEntryId(group.source_object_id.clone()))
            .map_err(|e| e.to_string())?;
    }
    items.sort_by_key(|group| {
        (
            Reverse(group.artifacts.len() + group.timelines.len()),
            Reverse(group.artifacts.len()),
            group_title(group),
        )
    });
    Ok(items)
}

pub(crate) fn build_rule_groups(
    conn: &Connection,
    artifacts: &[ArtifactRowDto],
    timelines: &[TimelineEventDto],
) -> Result<Vec<CorrelationRuleGroup>, String> {
    let files = crate::analysis_service::collect_file_entries(conn)?;
    let timeline_map = timelines.iter().fold(
        BTreeMap::<String, Vec<TimelineEventDto>>::new(),
        |mut acc, item| {
            acc.entry(item.source_object_id.clone())
                .or_default()
                .push(item.clone());
            acc
        },
    );
    // Parallel: each artifact's rule matching against all files is independent.
    // Use Mutex for shared group map — contention is low (~250 artifacts).
    let groups_map = Mutex::new(BTreeMap::<String, CorrelationRuleGroup>::new());

    artifacts.par_iter().for_each(|artifact| {
        for rule_match in build_artifact_rule_matches(&files, artifact) {
            let file_id = rule_match.file.id.0.clone();
            let mut groups = groups_map.lock().unwrap();
            let group = groups
                .entry(file_id.clone())
                .or_insert_with(|| CorrelationRuleGroup {
                    file: rule_match.file.clone(),
                    matches: Vec::new(),
                    timelines: timeline_map.get(&file_id).cloned().unwrap_or_default(),
                    timeline_signals: Vec::new(),
                });
            let exists = group.matches.iter().any(|existing| {
                existing.artifact.id == rule_match.artifact.id
                    && existing.kind == rule_match.kind
                    && existing.file.id == rule_match.file.id
            });
            if !exists {
                group.matches.push(rule_match);
            }
        }
    });

    let groups = groups_map.into_inner().unwrap();

    let mut items = groups.into_values().collect::<Vec<_>>();
    for group in &mut items {
        group.timeline_signals = derive_rule_timeline_signals(group, timelines);
        group.matches.sort_by_key(|rule| {
            (
                Reverse(confidence_rank(&rule.confidence)),
                rule.artifact.artifact_type.clone(),
                rule.artifact.id.clone(),
            )
        });
    }
    items.sort_by_key(|group| {
        (
            Reverse(confidence_rank(&rule_group_confidence(
                &group.matches,
                group.timelines.len() as u32,
                !group.timeline_signals.is_empty(),
            ))),
            Reverse(group.matches.len()),
            group.file.path.clone(),
        )
    });
    Ok(items)
}

// ── Group appenders ──

pub(crate) fn append_source_group(
    group: &CorrelationSourceGroup,
    node_map: &mut BTreeMap<String, CorrelationNodeDto>,
    edge_map: &mut BTreeMap<String, CorrelationEdgeDto>,
    clusters: &mut Vec<CorrelationClusterDto>,
    leads: &mut Vec<CorrelationLeadDto>,
) {
    let file_node_id = format!("file:{}", group.source_object_id);
    let artifact_count = group.artifacts.len() as u32;
    let timeline_count = group.timelines.len() as u32;
    let related_count = artifact_count + timeline_count;
    let confidence = group_confidence(artifact_count, timeline_count);
    let file_node = build_file_node(&file_node_id, group, related_count);
    let mut node_ids = vec![file_node.id.clone()];
    let mut edge_ids = Vec::new();
    let mut supporting_node_ids = Vec::new();
    let mut provenance = Vec::new();

    insert_node(node_map, file_node);

    for artifact in &group.artifacts {
        let artifact_node = build_artifact_node(artifact, related_count);
        let artifact_node_id = artifact_node.id.clone();
        let edge_id = format!("edge:{}:{}", artifact.id, group.source_object_id);

        insert_node(node_map, artifact_node);
        edge_map
            .entry(edge_id.clone())
            .or_insert(CorrelationEdgeDto {
                id: edge_id.clone(),
                kind: CorrelationEdgeKindDto::SourceReference,
                from_node_id: artifact_node_id.clone(),
                to_node_id: file_node_id.clone(),
                summary: format!("{} 引用同一 source object", artifact.artifact_type),
                confidence: CorrelationConfidenceDto::Direct,
            });

        node_ids.push(artifact_node_id.clone());
        edge_ids.push(edge_id);
        supporting_node_ids.push(artifact_node_id);
        provenance.push(build_artifact_provenance(artifact));
    }

    for timeline in &group.timelines {
        let timeline_node = build_timeline_node(timeline, related_count);
        let timeline_node_id = timeline_node.id.clone();
        let edge_id = format!("edge:{}:{}", timeline.id, group.source_object_id);

        insert_node(node_map, timeline_node);
        edge_map
            .entry(edge_id.clone())
            .or_insert(CorrelationEdgeDto {
                id: edge_id.clone(),
                kind: CorrelationEdgeKindDto::TemporalContext,
                from_node_id: timeline_node_id.clone(),
                to_node_id: file_node_id.clone(),
                summary: format!("{} 时间线事件命中同一 source object", timeline.event_type),
                confidence: if artifact_count > 0 {
                    CorrelationConfidenceDto::Direct
                } else {
                    CorrelationConfidenceDto::Strong
                },
            });

        node_ids.push(timeline_node_id.clone());
        edge_ids.push(edge_id);
        supporting_node_ids.push(timeline_node_id);
        provenance.push(build_timeline_provenance(timeline));
    }

    if let (Some(artifact), Some(timeline)) = (group.artifacts.first(), group.timelines.first()) {
        let edge_id = format!("edge:shared:{}:{}", artifact.id, timeline.id);
        edge_map
            .entry(edge_id.clone())
            .or_insert(CorrelationEdgeDto {
                id: edge_id.clone(),
                kind: CorrelationEdgeKindDto::SharedSourceObject,
                from_node_id: format!("artifact:{}", artifact.id),
                to_node_id: format!("timeline:{}", timeline.id),
                summary: "Artifact 与时间线共享同一 source object".to_string(),
                confidence: CorrelationConfidenceDto::Direct,
            });
        edge_ids.push(edge_id);
    }

    let title = group_title(group);
    let summary = group_summary(group);
    let caveats = group_caveats(group, artifact_count, timeline_count);
    let families = derive_source_group_families(group);

    clusters.push(CorrelationClusterDto {
        id: format!("cluster:{}", group.source_object_id),
        title: title.clone(),
        summary: summary.clone(),
        confidence: confidence.clone(),
        families: families.clone(),
        primary_file_id: group.source_object_id.clone(),
        artifact_count,
        timeline_count,
        node_ids,
        edge_ids,
        provenance: provenance.clone(),
    });

    leads.push(CorrelationLeadDto {
        id: format!("lead:{}", group.source_object_id),
        title: format!("{title} 形成关联线索"),
        summary,
        confidence,
        families,
        primary_file_id: group.source_object_id.clone(),
        supporting_node_ids,
        match_signals: source_group_match_signals(group),
        jumps: build_lead_jumps(group),
        provenance,
        caveats,
    });
}

pub(crate) fn append_rule_group(
    group: &CorrelationRuleGroup,
    node_map: &mut BTreeMap<String, CorrelationNodeDto>,
    edge_map: &mut BTreeMap<String, CorrelationEdgeDto>,
    clusters: &mut Vec<CorrelationClusterDto>,
    leads: &mut Vec<CorrelationLeadDto>,
) {
    let file_node_id = format!("file:{}", group.file.id.0);
    let related_count = (group.matches.len() + group.timelines.len()) as u32;
    insert_node(
        node_map,
        build_file_node_for_entry(&file_node_id, &group.file, related_count),
    );

    let mut node_ids = vec![file_node_id.clone()];
    let mut edge_ids = Vec::new();
    let mut supporting_node_ids = Vec::new();
    let mut provenance = Vec::new();
    let mut jumps = vec![CorrelationJumpTargetDto {
        route: "/files".to_string(),
        target_id: group.file.id.0.clone(),
        label: "查看文件".to_string(),
    }];

    for rule in &group.matches {
        let artifact_node = build_artifact_node(&rule.artifact, related_count);
        let artifact_node_id = artifact_node.id.clone();
        let edge_id = format!(
            "edge:rule:{}:{}:{}",
            rule.artifact.id,
            group.file.id.0,
            edge_kind_token(&rule.kind)
        );

        insert_node(node_map, artifact_node);
        edge_map
            .entry(edge_id.clone())
            .or_insert(CorrelationEdgeDto {
                id: edge_id.clone(),
                kind: rule.kind.clone(),
                from_node_id: artifact_node_id.clone(),
                to_node_id: file_node_id.clone(),
                summary: rule.summary.clone(),
                confidence: rule.confidence.clone(),
            });

        node_ids.push(artifact_node_id.clone());
        edge_ids.push(edge_id);
        supporting_node_ids.push(artifact_node_id);
        provenance.push(build_artifact_provenance(&rule.artifact));

        if jumps.len() == 1 {
            jumps.push(CorrelationJumpTargetDto {
                route: "/artifacts".to_string(),
                target_id: rule.artifact.id.clone(),
                label: "查看痕迹".to_string(),
            });
        }
    }

    let rule_group_confidence = rule_group_confidence(
        &group.matches,
        group.timelines.len() as u32,
        !group.timeline_signals.is_empty(),
    );
    for timeline in &group.timelines {
        let timeline_node = build_timeline_node(timeline, related_count);
        let timeline_node_id = timeline_node.id.clone();
        let edge_id = format!("edge:rule-timeline:{}:{}", timeline.id, group.file.id.0);

        insert_node(node_map, timeline_node);
        edge_map
            .entry(edge_id.clone())
            .or_insert(CorrelationEdgeDto {
                id: edge_id.clone(),
                kind: CorrelationEdgeKindDto::TemporalContext,
                from_node_id: timeline_node_id.clone(),
                to_node_id: file_node_id.clone(),
                summary: "关联文件时间线事件提供上下文".to_string(),
                confidence: rule_group_confidence.clone(),
            });

        node_ids.push(timeline_node_id.clone());
        edge_ids.push(edge_id);
        supporting_node_ids.push(timeline_node_id);
        provenance.push(build_timeline_provenance(timeline));
    }

    dedup_vec(&mut node_ids);
    dedup_vec(&mut edge_ids);
    dedup_vec(&mut supporting_node_ids);
    dedup_vec(&mut jumps);
    dedup_vec(&mut provenance);

    let confidence = rule_group_confidence;
    let title = group.file.name.clone();
    let summary = rule_group_summary(
        &group.matches,
        group.timelines.len() as u32,
        group.timeline_signals.len() as u32,
    );
    let caveats = rule_group_caveats(
        &group.matches,
        group.timelines.len() as u32,
        !group.timeline_signals.is_empty(),
    );
    let families = derive_rule_group_families(group);

    clusters.push(CorrelationClusterDto {
        id: format!("cluster:rules:{}", group.file.id.0),
        title: format!("{title} 规则命中"),
        summary: summary.clone(),
        confidence: confidence.clone(),
        families: families.clone(),
        primary_file_id: group.file.id.0.clone(),
        artifact_count: group.matches.len() as u32,
        timeline_count: group.timelines.len() as u32,
        node_ids,
        edge_ids,
        provenance: provenance.clone(),
    });

    leads.push(CorrelationLeadDto {
        id: format!("lead:rules:{}", group.file.id.0),
        title: format!("{title} 形成规则型关联线索"),
        summary,
        confidence,
        families,
        primary_file_id: group.file.id.0.clone(),
        supporting_node_ids,
        match_signals: rule_group_match_signals(
            &group.matches,
            group.timelines.len() as u32,
            &group.timeline_signals,
        ),
        jumps,
        provenance,
        caveats,
    });
}

// ── Node builders ──

pub(crate) fn build_file_node(
    file_node_id: &str,
    group: &CorrelationSourceGroup,
    related_count: u32,
) -> CorrelationNodeDto {
    if let Some(file) = &group.file {
        return build_file_node_for_entry(file_node_id, file, related_count);
    }

    CorrelationNodeDto {
        id: file_node_id.to_string(),
        kind: CorrelationNodeKindDto::File,
        title: group.source_object_id.clone(),
        subtitle: Some("未能映射到 file_entries，需回查原始工件。".to_string()),
        source_object_id: Some(group.source_object_id.clone()),
        related_count,
        badges: vec!["unresolved".to_string()],
        jumps: vec![CorrelationJumpTargetDto {
            route: "/files".to_string(),
            target_id: group.source_object_id.clone(),
            label: "打开文件浏览".to_string(),
        }],
    }
}

pub(crate) fn build_file_node_for_entry(
    file_node_id: &str,
    file: &FileEntry,
    related_count: u32,
) -> CorrelationNodeDto {
    let title = if file.entry_type == domain::EntryType::Directory {
        format!("{}/", file.name)
    } else {
        file.name.clone()
    };
    let mut badges = Vec::new();
    if file.deleted {
        badges.push("deleted".to_string());
    }
    if file.hidden {
        badges.push("hidden".to_string());
    }
    if file.system {
        badges.push("system".to_string());
    }

    CorrelationNodeDto {
        id: file_node_id.to_string(),
        kind: CorrelationNodeKindDto::File,
        title,
        subtitle: Some(file.path.clone()),
        source_object_id: Some(file.id.0.clone()),
        related_count,
        badges,
        jumps: vec![CorrelationJumpTargetDto {
            route: "/files".to_string(),
            target_id: file.id.0.clone(),
            label: "打开文件浏览".to_string(),
        }],
    }
}

pub(crate) fn build_artifact_node(
    artifact: &ArtifactRowDto,
    related_count: u32,
) -> CorrelationNodeDto {
    CorrelationNodeDto {
        id: format!("artifact:{}", artifact.id),
        kind: CorrelationNodeKindDto::Artifact,
        title: artifact.title.clone(),
        subtitle: Some(artifact.summary.clone()),
        source_object_id: artifact.source_object_id.clone(),
        related_count,
        badges: vec![artifact.artifact_type.clone()],
        jumps: vec![CorrelationJumpTargetDto {
            route: "/artifacts".to_string(),
            target_id: artifact.id.clone(),
            label: "打开痕迹分析".to_string(),
        }],
    }
}

pub(crate) fn build_timeline_node(
    timeline: &TimelineEventDto,
    related_count: u32,
) -> CorrelationNodeDto {
    CorrelationNodeDto {
        id: format!("timeline:{}", timeline.id),
        kind: CorrelationNodeKindDto::TimelineEvent,
        title: timeline.title.clone(),
        subtitle: Some(format!("{} · {}", timeline.ts, timeline.event_type)),
        source_object_id: Some(timeline.source_object_id.clone()),
        related_count,
        badges: vec![timeline.event_type.clone()],
        jumps: vec![CorrelationJumpTargetDto {
            route: "/timeline".to_string(),
            target_id: timeline.id.clone(),
            label: "打开时间线".to_string(),
        }],
    }
}

// ── Provenance builders ──

pub(crate) fn build_artifact_provenance(artifact: &ArtifactRowDto) -> CorrelationProvenanceDto {
    CorrelationProvenanceDto {
        source_kind: "artifact".to_string(),
        source_record_id: artifact.id.clone(),
        source_label: artifact.artifact_type.clone(),
        producer: artifact.extractor_id.clone(),
        producer_version: artifact.extractor_version.clone(),
        guarantee_level: artifact_guarantee_level(&artifact.artifact_type),
        warning_summary: Vec::new(),
    }
}

pub(crate) fn build_timeline_provenance(timeline: &TimelineEventDto) -> CorrelationProvenanceDto {
    CorrelationProvenanceDto {
        source_kind: "timeline".to_string(),
        source_record_id: timeline.id.clone(),
        source_label: timeline.event_type.clone(),
        producer: timeline.parser_id.clone(),
        producer_version: timeline.parser_version.clone(),
        guarantee_level: timeline_guarantee_level(timeline.parser_id.as_deref()),
        warning_summary: Vec::new(),
    }
}

// ── Jump / title / summary / caveats builders ──

pub(crate) fn build_lead_jumps(group: &CorrelationSourceGroup) -> Vec<CorrelationJumpTargetDto> {
    let mut jumps = vec![CorrelationJumpTargetDto {
        route: "/files".to_string(),
        target_id: group.source_object_id.clone(),
        label: "查看文件".to_string(),
    }];
    if let Some(artifact) = group.artifacts.first() {
        jumps.push(CorrelationJumpTargetDto {
            route: "/artifacts".to_string(),
            target_id: artifact.id.clone(),
            label: "查看痕迹".to_string(),
        });
    }
    if let Some(timeline) = group.timelines.first() {
        jumps.push(CorrelationJumpTargetDto {
            route: "/timeline".to_string(),
            target_id: timeline.id.clone(),
            label: "查看时间线".to_string(),
        });
    }
    jumps
}

pub(crate) fn group_title(group: &CorrelationSourceGroup) -> String {
    group
        .file
        .as_ref()
        .map(|file| file.name.clone())
        .unwrap_or_else(|| group.source_object_id.clone())
}

pub(crate) fn group_summary(group: &CorrelationSourceGroup) -> String {
    let artifact_count = group.artifacts.len();
    let timeline_count = group.timelines.len();
    match (artifact_count, timeline_count) {
        (0, timeline_count) => format!("同一 source object 命中 {timeline_count} 条时间线事件。"),
        (artifact_count, 0) => format!("同一 source object 命中 {artifact_count} 条痕迹记录。"),
        (artifact_count, timeline_count) => {
            format!("同一 source object 聚合 {artifact_count} 条痕迹记录与 {timeline_count} 条时间线事件。")
        }
    }
}

pub(crate) fn group_caveats(
    group: &CorrelationSourceGroup,
    artifact_count: u32,
    timeline_count: u32,
) -> Vec<String> {
    let mut caveats = Vec::new();
    if group.file.is_none() {
        caveats.push(
            "source_object_id 未映射到 file_entries，需结合原始工件与导入链路复核。".to_string(),
        );
    }
    if artifact_count == 0 || timeline_count == 0 {
        caveats.push("当前仅形成单侧证据命中，尚未完成跨工件交叉验证。".to_string());
    }
    if timeline_count > 0 {
        caveats.push("时间线命中可能来自聚合投影，解释时需回跳原始事件。".to_string());
    }
    caveats
}

// ── Match signals ──

pub(crate) fn source_group_match_signals(group: &CorrelationSourceGroup) -> Vec<String> {
    let mut signals = Vec::new();
    if !group.artifacts.is_empty() {
        signals.push(format!(
            "同一 source object 命中 {} 条 artifact",
            group.artifacts.len()
        ));
    }
    if !group.timelines.is_empty() {
        signals.push(format!(
            "同一 source object 命中 {} 条 timeline",
            group.timelines.len()
        ));
    }
    signals
}

pub(crate) fn rule_group_match_signals(
    matches: &[CorrelationRuleMatch],
    timeline_count: u32,
    timeline_signals: &[String],
) -> Vec<String> {
    let mut signals = matches
        .iter()
        .map(|item| item.summary.clone())
        .collect::<Vec<_>>();
    if timeline_count > 0 {
        signals.push("关联文件时间线事件提供上下文".to_string());
    }
    signals.extend(timeline_signals.iter().cloned());
    dedup_vec(&mut signals);
    signals
}

pub(crate) fn rule_group_summary(
    matches: &[CorrelationRuleMatch],
    timeline_count: u32,
    proximity_timeline_count: u32,
) -> String {
    let mut path_matches = 0usize;
    let mut name_matches = 0usize;
    let mut recovered_matches = 0usize;
    let mut artifact_types = BTreeSet::new();

    for item in matches {
        artifact_types.insert(item.artifact.artifact_type.clone());
        match item.kind {
            CorrelationEdgeKindDto::PathMatch => path_matches += 1,
            CorrelationEdgeKindDto::NameMatch => name_matches += 1,
            CorrelationEdgeKindDto::RecoveredOriginalPath => recovered_matches += 1,
            _ => {}
        }
    }

    let families = artifact_types.into_iter().collect::<Vec<_>>().join(" / ");
    format!(
        "{families} 规则命中 {} 条记录（路径 {}，名称 {}，原路径恢复 {}，自身时间线 {}，邻近时间线 {}）。",
        matches.len(),
        path_matches,
        name_matches,
        recovered_matches,
        timeline_count,
        proximity_timeline_count
    )
}

pub(crate) fn rule_group_caveats(
    matches: &[CorrelationRuleMatch],
    timeline_count: u32,
    has_proximity_timeline: bool,
) -> Vec<String> {
    let mut caveats = matches
        .iter()
        .map(|item| item.caveat.clone())
        .collect::<Vec<_>>();
    if timeline_count == 0 && !has_proximity_timeline {
        caveats.push("当前规则命中尚未获得同文件时间线佐证。".to_string());
    }
    dedup_vec(&mut caveats);
    caveats
}

// ── Guarantee / confidence ──

pub(crate) fn artifact_guarantee_level(artifact_type: &str) -> VerificationGuaranteeLevelDto {
    match artifact_type {
        "Prefetch" | "LNK" | "Registry" | "RegistryValue" | "RecycleBin" => {
            VerificationGuaranteeLevelDto::BestEffort
        }
        "BrowserDownload" | "BrowserHistory" | "EmailMessage" => {
            VerificationGuaranteeLevelDto::Experimental
        }
        _ => VerificationGuaranteeLevelDto::Experimental,
    }
}

pub(crate) fn timeline_guarantee_level(parser_id: Option<&str>) -> VerificationGuaranteeLevelDto {
    match parser_id {
        Some(parser_id) if parser_id.starts_with("timeline.") => {
            VerificationGuaranteeLevelDto::BestEffort
        }
        Some(parser_id) if parser_id.starts_with("evtx.") => {
            VerificationGuaranteeLevelDto::BestEffort
        }
        _ => VerificationGuaranteeLevelDto::Experimental,
    }
}

pub(crate) fn group_confidence(
    artifact_count: u32,
    timeline_count: u32,
) -> CorrelationConfidenceDto {
    if artifact_count > 0 && timeline_count > 0 {
        CorrelationConfidenceDto::Direct
    } else if artifact_count + timeline_count >= 3 {
        CorrelationConfidenceDto::Strong
    } else if artifact_count + timeline_count >= 1 {
        CorrelationConfidenceDto::Weak
    } else {
        CorrelationConfidenceDto::Heuristic
    }
}

pub(crate) fn rule_group_confidence(
    matches: &[CorrelationRuleMatch],
    timeline_count: u32,
    has_proximity_timeline: bool,
) -> CorrelationConfidenceDto {
    if matches
        .iter()
        .any(|item| item.kind == CorrelationEdgeKindDto::RecoveredOriginalPath)
    {
        return CorrelationConfidenceDto::Direct;
    }
    if matches
        .iter()
        .any(|item| item.kind == CorrelationEdgeKindDto::PathMatch)
    {
        return CorrelationConfidenceDto::Direct;
    }
    if (timeline_count > 0 || has_proximity_timeline) && !matches.is_empty() {
        return CorrelationConfidenceDto::Strong;
    }
    if matches.len() >= 2
        || matches
            .iter()
            .any(|item| item.confidence == CorrelationConfidenceDto::Strong)
    {
        return CorrelationConfidenceDto::Strong;
    }
    if !matches.is_empty() {
        return CorrelationConfidenceDto::Weak;
    }
    CorrelationConfidenceDto::Heuristic
}

// ── Timeline signals derivation ──

pub(crate) fn derive_rule_timeline_signals(
    group: &CorrelationRuleGroup,
    all_timelines: &[TimelineEventDto],
) -> Vec<String> {
    let mut signals = Vec::new();

    if !group.timelines.is_empty() {
        signals.push(format!(
            "关联文件自身已有 {} 条 timeline 事件",
            group.timelines.len()
        ));
    }

    let artifact_times = group
        .matches
        .iter()
        .flat_map(rule_match_timestamps)
        .collect::<Vec<_>>();

    if artifact_times.is_empty() {
        return signals;
    }

    let target_path_keys = group
        .matches
        .iter()
        .flat_map(rule_match_paths)
        .map(|value| path_suffix_key(&value))
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();
    let text_needles = group
        .matches
        .iter()
        .flat_map(rule_match_text_needles)
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();

    if target_path_keys.is_empty() && text_needles.is_empty() {
        return signals;
    }

    let mut related = all_timelines
        .iter()
        .filter(|timeline| timeline.source_object_id != group.file.id.0)
        .filter_map(|timeline| {
            let timeline_ts = parse_rfc3339_utc(&timeline.ts)?;
            let within_window = artifact_times.iter().any(|artifact_ts| {
                (timeline_ts.timestamp() - artifact_ts.timestamp()).abs()
                    <= RULE_TIMELINE_PROXIMITY_WINDOW_SECS
            });
            if !within_window {
                return None;
            }

            let timeline_paths = timeline_path_candidates(timeline);
            let path_hit = timeline_paths.into_iter().any(|candidate| {
                let suffix = path_suffix_key(&candidate);
                !suffix.is_empty() && target_path_keys.contains(&suffix)
            });
            let text_hit = timeline_text_candidates(timeline)
                .into_iter()
                .any(|candidate| {
                    let normalized = candidate.to_ascii_lowercase();
                    text_needles
                        .iter()
                        .any(|needle| !needle.is_empty() && normalized.contains(needle))
                });
            if !path_hit && !text_hit {
                return None;
            }

            Some(format!("{} @ {}", timeline.event_type, timeline.ts))
        })
        .collect::<Vec<_>>();

    related.sort();
    related.truncate(RULE_TIMELINE_CONTEXT_LIMIT);
    for item in related {
        signals.push(format!("邻近时间线命中 {item}"));
    }

    signals
}

// ── Family coverage ──

pub(crate) fn build_family_coverage(
    leads: &[CorrelationLeadDto],
    clusters: &[CorrelationClusterDto],
) -> Vec<CorrelationFamilyCoverageDto> {
    CORRELATION_RULE_FAMILIES
        .iter()
        .map(|(family, display_name)| {
            let family_token = family.to_ascii_lowercase();
            let related_leads = leads
                .iter()
                .filter(|lead| {
                    has_family(&lead.families, family)
                        || lead.provenance.iter().any(|item| {
                            item.source_label.eq_ignore_ascii_case(family)
                                || item.source_kind.eq_ignore_ascii_case(family)
                                || item
                                    .producer
                                    .as_deref()
                                    .map(|producer| {
                                        producer.to_ascii_lowercase().contains(&family_token)
                                    })
                                    .unwrap_or(false)
                        })
                        || lead
                            .match_signals
                            .iter()
                            .any(|signal| signal.to_ascii_lowercase().contains(&family_token))
                })
                .collect::<Vec<_>>();

            let cluster_count = clusters
                .iter()
                .filter(|cluster| {
                    has_family(&cluster.families, family)
                        || cluster.provenance.iter().any(|item| {
                            item.source_label.eq_ignore_ascii_case(family)
                                || item.source_kind.eq_ignore_ascii_case(family)
                                || item
                                    .producer
                                    .as_deref()
                                    .map(|producer| {
                                        producer.to_ascii_lowercase().contains(&family_token)
                                    })
                                    .unwrap_or(false)
                        })
                        || cluster.summary.to_ascii_lowercase().contains(&family_token)
                })
                .count() as u32;

            let lead_count = related_leads.len() as u32;
            let high_confidence_lead_count = related_leads
                .iter()
                .filter(|lead| {
                    matches!(
                        lead.confidence,
                        CorrelationConfidenceDto::Direct | CorrelationConfidenceDto::Strong
                    )
                })
                .count() as u32;
            let review_lead_count = related_leads
                .iter()
                .filter(|lead| {
                    !lead.caveats.is_empty()
                        || matches!(
                            lead.confidence,
                            CorrelationConfidenceDto::Weak | CorrelationConfidenceDto::Heuristic
                        )
                })
                .count() as u32;
            let mut sample_signals = related_leads
                .iter()
                .flat_map(|lead| lead.match_signals.iter().cloned())
                .filter(|signal| signal.to_ascii_lowercase().contains(&family_token))
                .take(3)
                .collect::<Vec<_>>();
            if sample_signals.is_empty() {
                sample_signals = related_leads
                    .iter()
                    .flat_map(|lead| lead.match_signals.iter().cloned())
                    .take(3)
                    .collect::<Vec<_>>();
            }

            let status = if lead_count == 0 {
                CorrelationCoverageStatusDto::Missing
            } else if high_confidence_lead_count > 0 {
                CorrelationCoverageStatusDto::Covered
            } else {
                CorrelationCoverageStatusDto::Review
            };

            CorrelationFamilyCoverageDto {
                family: (*family).to_string(),
                display_name: (*display_name).to_string(),
                status,
                lead_count,
                high_confidence_lead_count,
                review_lead_count,
                cluster_count,
                sample_signals,
            }
        })
        .collect()
}

pub(crate) fn derive_source_group_families(group: &CorrelationSourceGroup) -> Vec<String> {
    let mut families = group
        .artifacts
        .iter()
        .filter_map(|artifact| artifact_family(&artifact.artifact_type))
        .collect::<Vec<_>>();
    dedup_vec(&mut families);
    families
}

pub(crate) fn derive_rule_group_families(group: &CorrelationRuleGroup) -> Vec<String> {
    let mut families = group
        .matches
        .iter()
        .filter_map(|item| artifact_family(&item.artifact.artifact_type))
        .collect::<Vec<_>>();
    dedup_vec(&mut families);
    families
}

// ── Persist CorrelatesWith edges ──

/// Persist CorrelatesWith graph edges from correlation leads into the investigative graph.
///
/// For each lead, creates CorrelatesWith edges linking each supporting artifact node
/// to the lead's primary file node. The edge carries confidence and rule provenance
/// (serialised match signals).
fn persist_correlation_edges(
    conn: &Connection,
    leads: &[CorrelationLeadDto],
) -> Result<(), String> {
    if leads.is_empty() {
        return Ok(());
    }

    // Resolve case_id from the artifacts table
    let case_id: String = match conn.query_row(
        "SELECT DISTINCT case_id FROM artifacts LIMIT 1",
        [],
        |row| row.get::<_, String>(0),
    ) {
        Ok(id) => id,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(()),
        Err(e) => return Err(format!("resolve case_id for correlation edges: {e}")),
    };

    let graph_repo = GraphRepo::new(conn);
    let now = Utc::now().to_rfc3339();
    let mut edges: Vec<GraphEdge> = Vec::new();

    for lead in leads {
        let confidence = map_correlation_confidence(&lead.confidence);
        let provenance = build_correlation_provenance(lead);

        for node_id in &lead.supporting_node_ids {
            // Only process artifact nodes (prefixed "artifact:")
            let artifact_id = match node_id.strip_prefix("artifact:") {
                Some(raw) => raw,
                None => continue,
            };

            edges.push(GraphEdge {
                id: format!("correlates_with:{artifact_id}:{}", lead.primary_file_id),
                case_id: case_id.clone(),
                source_id: artifact_id.to_string(),
                target_id: lead.primary_file_id.clone(),
                edge_type: EdgeType::CorrelatesWith,
                confidence: Some(confidence),
                provenance: Some(provenance.clone()),
                created_at: now.clone(),
            });
        }
    }

    if !edges.is_empty() {
        // Non-fatal: correlation edges reference artifact/file nodes that may
        // not exist in the graph yet (e.g. partial import, test case). Missing
        // edges are logged but do not block the correlation operation.
        if let Err(e) = graph_repo.insert_edges_batch(&edges) {
            tracing::warn!("correlation graph edge insert (non-fatal): {e}");
        }
    }

    Ok(())
}

/// Map CorrelationConfidenceDto to a numeric confidence value in [0.0, 1.0].
fn map_correlation_confidence(confidence: &CorrelationConfidenceDto) -> f64 {
    match confidence {
        CorrelationConfidenceDto::Direct => 1.0,
        CorrelationConfidenceDto::Strong => 0.9,
        CorrelationConfidenceDto::Weak => 0.5,
        CorrelationConfidenceDto::Heuristic => 0.3,
    }
}

/// Build a JSON provenance string from a correlation lead's match signals.
fn build_correlation_provenance(lead: &CorrelationLeadDto) -> String {
    let signals = if lead.match_signals.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::Array(
            lead.match_signals
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect(),
        )
    };

    serde_json::json!({
        "kind": "correlation_rule",
        "lead_id": lead.id,
        "match_signals": signals,
        "families": lead.families,
    })
    .to_string()
}
