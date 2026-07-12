use super::super::CorrelationError;
use chrono::Utc;
use domain::{EdgeType, GraphEdge};
use persistence_sqlite::repositories::{correlation_repo, graph_repo::GraphRepo};
use rusqlite::Connection;
use transport::dto::{CorrelationConfidenceDto, CorrelationLeadDto};

pub(crate) fn persist_correlation_edges(
    conn: &Connection,
    leads: &[CorrelationLeadDto],
) -> Result<(), CorrelationError> {
    if leads.is_empty() {
        return Ok(());
    }
    let Some(case_id) = resolve_case_id(conn)? else {
        return Ok(());
    };
    let edges = build_edges(leads, &case_id, &Utc::now().to_rfc3339());
    if edges.is_empty() {
        return Ok(());
    }
    if let Err(error) = GraphRepo::new(conn).insert_edges_batch(&edges) {
        tracing::warn!("correlation graph edge insert (non-fatal): {error}");
    }
    Ok(())
}

fn resolve_case_id(conn: &Connection) -> Result<Option<String>, CorrelationError> {
    correlation_repo::resolve_case_id(conn).map_err(|error| {
        CorrelationError::Other(format!("resolve case_id for correlation edges: {error}"))
    })
}

fn build_edges(leads: &[CorrelationLeadDto], case_id: &str, created_at: &str) -> Vec<GraphEdge> {
    leads
        .iter()
        .flat_map(|lead| {
            let confidence = map_correlation_confidence(&lead.confidence);
            let provenance = build_correlation_provenance(lead);
            lead.supporting_node_ids.iter().filter_map(move |node_id| {
                let artifact_id = node_id.strip_prefix("artifact:")?;
                Some(GraphEdge {
                    id: format!("correlates_with:{artifact_id}:{}", lead.primary_file_id),
                    case_id: case_id.to_string(),
                    source_id: artifact_id.to_string(),
                    target_id: lead.primary_file_id.clone(),
                    edge_type: EdgeType::CorrelatesWith,
                    confidence: Some(confidence),
                    provenance: Some(provenance.clone()),
                    created_at: created_at.to_string(),
                })
            })
        })
        .collect()
}

fn map_correlation_confidence(confidence: &CorrelationConfidenceDto) -> f64 {
    match confidence {
        CorrelationConfidenceDto::Direct => 1.0,
        CorrelationConfidenceDto::Strong => 0.9,
        CorrelationConfidenceDto::Weak => 0.5,
        CorrelationConfidenceDto::Heuristic => 0.3,
    }
}

fn build_correlation_provenance(lead: &CorrelationLeadDto) -> String {
    let signals = if lead.match_signals.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::Array(
            lead.match_signals
                .iter()
                .cloned()
                .map(serde_json::Value::String)
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
