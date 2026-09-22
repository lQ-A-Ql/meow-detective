use domain::CaseId;
use persistence_sqlite::repositories::infrastructure_graph_repo::InfrastructureGraphRepo;
use transport::dto::{
    InfrastructureGraphDto, InfrastructureGraphEdgeDto, InfrastructureGraphNodeDto,
};

pub fn get_infrastructure_graph(
    connection: &rusqlite::Connection,
    case_id: &CaseId,
) -> Result<InfrastructureGraphDto, persistence_sqlite::DbError> {
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
    Ok(InfrastructureGraphDto { nodes, edges })
}
