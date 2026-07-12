use super::super::confidence_rank;
use super::super::coverage::build_family_coverage;
use chrono::Utc;
use std::cmp::Reverse;
use transport::dto::{
    CorrelationClusterDto, CorrelationEdgeDto, CorrelationLeadDto, CorrelationNodeDto,
    CorrelationProvenanceDto, CorrelationSnapshotDto,
};

pub(crate) fn empty_snapshot() -> CorrelationSnapshotDto {
    CorrelationSnapshotDto {
        generated_at: Utc::now().to_rfc3339(),
        node_count: 0,
        edge_count: 0,
        cluster_count: 0,
        lead_count: 0,
        family_coverage: Vec::new(),
        nodes: Vec::new(),
        edges: Vec::new(),
        clusters: Vec::new(),
        leads: Vec::new(),
    }
}

pub(crate) fn merge_source_snapshot(
    merged: &mut CorrelationSnapshotDto,
    snapshot: CorrelationSnapshotDto,
    data_source_id: &domain::DataSourceId,
) {
    merged.nodes.extend(
        snapshot
            .nodes
            .into_iter()
            .map(|node| scope_correlation_node(node, data_source_id)),
    );
    merged.edges.extend(
        snapshot
            .edges
            .into_iter()
            .map(|edge| scope_correlation_edge(edge, data_source_id)),
    );
    merged.clusters.extend(
        snapshot
            .clusters
            .into_iter()
            .map(|cluster| scope_correlation_cluster(cluster, data_source_id)),
    );
    merged.leads.extend(
        snapshot
            .leads
            .into_iter()
            .map(|lead| scope_correlation_lead(lead, data_source_id)),
    );
}

pub(crate) fn finalize_snapshot_counts(snapshot: &mut CorrelationSnapshotDto) {
    snapshot.nodes.sort_by(|left, right| left.id.cmp(&right.id));
    snapshot.edges.sort_by(|left, right| left.id.cmp(&right.id));
    snapshot
        .clusters
        .sort_by(|left, right| left.id.cmp(&right.id));
    snapshot.leads.sort_by_key(|lead| {
        (
            Reverse(confidence_rank(&lead.confidence)),
            Reverse(lead.supporting_node_ids.len()),
            lead.title.clone(),
        )
    });
    snapshot.node_count = snapshot.nodes.len() as u32;
    snapshot.edge_count = snapshot.edges.len() as u32;
    snapshot.cluster_count = snapshot.clusters.len() as u32;
    snapshot.lead_count = snapshot.leads.len() as u32;
    snapshot.family_coverage = build_family_coverage(&snapshot.leads, &snapshot.clusters);
    snapshot.generated_at = Utc::now().to_rfc3339();
}

fn scope_correlation_node(
    mut node: CorrelationNodeDto,
    data_source_id: &domain::DataSourceId,
) -> CorrelationNodeDto {
    node.id = scope_node_id(&node.id, data_source_id);
    node.source_object_id = node
        .source_object_id
        .map(|id| scope_record_id(&id, data_source_id));
    for jump in &mut node.jumps {
        jump.target_id = scope_jump_target(&jump.route, &jump.target_id, data_source_id);
    }
    node
}

fn scope_correlation_edge(
    mut edge: CorrelationEdgeDto,
    data_source_id: &domain::DataSourceId,
) -> CorrelationEdgeDto {
    edge.id = scoped_id(data_source_id, &edge.id);
    edge.from_node_id = scope_node_id(&edge.from_node_id, data_source_id);
    edge.to_node_id = scope_node_id(&edge.to_node_id, data_source_id);
    edge
}

fn scope_correlation_cluster(
    mut cluster: CorrelationClusterDto,
    data_source_id: &domain::DataSourceId,
) -> CorrelationClusterDto {
    cluster.id = scoped_id(data_source_id, &cluster.id);
    cluster.primary_file_id = scope_record_id(&cluster.primary_file_id, data_source_id);
    cluster.node_ids = cluster
        .node_ids
        .into_iter()
        .map(|id| scope_node_id(&id, data_source_id))
        .collect();
    cluster.edge_ids = cluster
        .edge_ids
        .into_iter()
        .map(|id| scoped_id(data_source_id, &id))
        .collect();
    for provenance in &mut cluster.provenance {
        scope_provenance(provenance, data_source_id);
    }
    cluster
}

fn scope_correlation_lead(
    mut lead: CorrelationLeadDto,
    data_source_id: &domain::DataSourceId,
) -> CorrelationLeadDto {
    lead.id = scoped_id(data_source_id, &lead.id);
    lead.primary_file_id = scope_record_id(&lead.primary_file_id, data_source_id);
    lead.supporting_node_ids = lead
        .supporting_node_ids
        .into_iter()
        .map(|id| scope_node_id(&id, data_source_id))
        .collect();
    for jump in &mut lead.jumps {
        jump.target_id = scope_jump_target(&jump.route, &jump.target_id, data_source_id);
    }
    for provenance in &mut lead.provenance {
        scope_provenance(provenance, data_source_id);
    }
    lead
}

fn scope_provenance(
    provenance: &mut CorrelationProvenanceDto,
    data_source_id: &domain::DataSourceId,
) {
    if matches!(provenance.source_kind.as_str(), "artifact" | "timeline") {
        provenance.source_record_id = scope_record_id(&provenance.source_record_id, data_source_id);
    }
}

fn scope_node_id(id: &str, data_source_id: &domain::DataSourceId) -> String {
    if let Some((kind, local_id)) = id.split_once(':') {
        format!("{kind}:{}", scoped_id(data_source_id, local_id))
    } else {
        scoped_id(data_source_id, id)
    }
}

fn scope_record_id(id: &str, data_source_id: &domain::DataSourceId) -> String {
    if id.starts_with("ds:") {
        id.to_string()
    } else {
        scoped_id(data_source_id, id)
    }
}

fn scope_jump_target(
    route: &str,
    target_id: &str,
    data_source_id: &domain::DataSourceId,
) -> String {
    match route {
        "/files" | "/artifacts" | "/timeline" => scope_record_id(target_id, data_source_id),
        _ => target_id.to_string(),
    }
}

fn scoped_id(data_source_id: &domain::DataSourceId, id: &str) -> String {
    crate::source_db::encode_source_scoped_id(data_source_id, id)
}
