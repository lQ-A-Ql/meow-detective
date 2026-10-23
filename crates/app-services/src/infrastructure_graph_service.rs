use domain::CaseId;
use persistence_sqlite::repositories::infrastructure_graph_repo::InfrastructureGraphRepo;
use std::sync::atomic::{AtomicBool, Ordering};
use transport::dto::{
    InfrastructureGraphDto, InfrastructureGraphEdgeDto, InfrastructureGraphNodeDto,
    InfrastructureNetworkFactDto,
};

mod kubernetes_network_facts;
mod network_facts;
mod platform_facts;

#[derive(Debug, thiserror::Error)]
pub enum InfrastructureProjectionError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("infrastructure projection cancelled")]
    Cancelled,
}

impl transport::ServiceErrorCategory for InfrastructureProjectionError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) => transport::ErrorCategory::Io,
            Self::Cancelled => transport::ErrorCategory::Cancelled,
        }
    }
}

pub fn rebuild_infrastructure_graph_for_case(
    connection: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    cancel_token: &AtomicBool,
) -> Result<(), InfrastructureProjectionError> {
    ensure_not_cancelled(cancel_token)?;
    get_infrastructure_graph(connection, case_root, case_id)?;
    ensure_not_cancelled(cancel_token)
}

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> Result<(), InfrastructureProjectionError> {
    if cancel_token.load(Ordering::Relaxed) {
        Err(InfrastructureProjectionError::Cancelled)
    } else {
        Ok(())
    }
}

pub fn get_infrastructure_graph(
    connection: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
) -> Result<InfrastructureGraphDto, persistence_sqlite::DbError> {
    let network_diagnostics = network_facts::refresh_network_facts(connection, case_root, case_id)?;
    let repo = InfrastructureGraphRepo::new(connection);
    let nodes = repo
        .list_nodes(&case_id.0)?
        .into_iter()
        .map(|node| InfrastructureGraphNodeDto {
            id: node.id,
            domain: node.domain,
            kind: node.kind,
            name: node.name,
            status: node.status,
            confidence: node.confidence,
            provenance_json: node.provenance_json,
            version: node.version,
        })
        .collect();
    let edges = repo
        .list_edges(&case_id.0)?
        .into_iter()
        .map(|edge| InfrastructureGraphEdgeDto {
            source_id: edge.source_id,
            target_id: edge.target_id,
            relation_kind: edge.relation_kind,
            confidence: edge.confidence,
            provenance_json: edge.provenance_json,
        })
        .collect();
    let network_facts = persistence_sqlite::repositories::infrastructure_network_fact_repo::InfrastructureNetworkFactRepo::new(connection)
        .list_for_case(&case_id.0)?
        .into_iter()
        .map(|fact| InfrastructureNetworkFactDto {
            id: fact.id,
            data_source_id: fact.data_source_id,
            environment_object_id: fact.environment_object_id,
            file_id: fact.file_id,
            source_path: fact.source_path,
            line_number: fact.line_number,
            fact_kind: fact.fact_kind,
            subject: fact.subject,
            value: fact.value,
            assertion_kind: fact.assertion_kind,
            confidence: fact.confidence,
            parser: fact.parser,
        })
        .collect();
    Ok(InfrastructureGraphDto {
        nodes,
        edges,
        network_facts,
        network_diagnostics,
    })
}

pub fn get_infrastructure_graph_read_only(
    connection: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
) -> Result<InfrastructureGraphDto, persistence_sqlite::DbError> {
    let (computed_facts, network_diagnostics) =
        network_facts::collect_network_facts(connection, case_root, case_id)?;
    let repo = InfrastructureGraphRepo::new(connection);
    let nodes = repo
        .list_nodes(&case_id.0)?
        .into_iter()
        .map(|node| InfrastructureGraphNodeDto {
            id: node.id,
            domain: node.domain,
            kind: node.kind,
            name: node.name,
            status: node.status,
            confidence: node.confidence,
            provenance_json: node.provenance_json,
            version: node.version,
        })
        .collect();
    let edges = repo
        .list_edges(&case_id.0)?
        .into_iter()
        .map(|edge| InfrastructureGraphEdgeDto {
            source_id: edge.source_id,
            target_id: edge.target_id,
            relation_kind: edge.relation_kind,
            confidence: edge.confidence,
            provenance_json: edge.provenance_json,
        })
        .collect();
    let network_facts = computed_facts
        .into_iter()
        .map(|fact| InfrastructureNetworkFactDto {
            id: fact.id,
            data_source_id: fact.data_source_id,
            environment_object_id: fact.environment_object_id,
            file_id: fact.file_id,
            source_path: fact.source_path,
            line_number: fact.line_number,
            fact_kind: fact.fact_kind,
            subject: fact.subject,
            value: fact.value,
            assertion_kind: fact.assertion_kind,
            confidence: fact.confidence,
            parser: fact.parser,
        })
        .collect();
    Ok(InfrastructureGraphDto {
        nodes,
        edges,
        network_facts,
        network_diagnostics,
    })
}
