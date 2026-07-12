use super::super::confidence_rank;
use super::super::coverage::build_family_coverage;
use chrono::Utc;
use std::cmp::Reverse;
use transport::dto::CorrelationSnapshotDto;

pub(super) fn finalize_local_snapshot(snapshot: &mut CorrelationSnapshotDto) {
    snapshot
        .nodes
        .sort_by_key(|node| (node.kind.clone(), node.title.clone(), node.id.clone()));
    snapshot
        .edges
        .sort_by_key(|edge| (Reverse(confidence_rank(&edge.confidence)), edge.id.clone()));
    snapshot.leads.sort_by_key(|lead| {
        (
            Reverse(confidence_rank(&lead.confidence)),
            Reverse(lead.supporting_node_ids.len()),
            lead.title.clone(),
        )
    });
    snapshot.clusters.sort_by_key(|cluster| {
        (
            Reverse(confidence_rank(&cluster.confidence)),
            Reverse(cluster.node_ids.len()),
            cluster.title.clone(),
        )
    });
    snapshot.node_count = snapshot.nodes.len() as u32;
    snapshot.edge_count = snapshot.edges.len() as u32;
    snapshot.cluster_count = snapshot.clusters.len() as u32;
    snapshot.lead_count = snapshot.leads.len() as u32;
    snapshot.family_coverage = build_family_coverage(&snapshot.leads, &snapshot.clusters);
    snapshot.generated_at = Utc::now().to_rfc3339();
}
