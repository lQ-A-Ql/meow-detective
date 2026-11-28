use domain::{
    structure::{StructureNodeKind, StructureRelationKind},
    CaseId,
};
use transport::dto::{InfrastructureGraphEdgeDto, InfrastructureGraphNodeDto};

pub(super) fn append(
    conn: &rusqlite::Connection,
    root: &std::path::Path,
    case_id: &CaseId,
    nodes: &mut Vec<InfrastructureGraphNodeDto>,
    edges: &mut Vec<InfrastructureGraphEdgeDto>,
) -> persistence_sqlite::DbResult<()> {
    let graph =
        source_reconstruction::structure_facts::get_structure_fact_graph(conn, root, case_id, &[])
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    let by_id = graph
        .nodes
        .iter()
        .map(|node| (&node.id, node))
        .collect::<std::collections::BTreeMap<_, _>>();
    for edge in &graph.edges {
        let source = by_id.get(&edge.from).expect("validated graph source");
        let relation = match edge.kind {
            StructureRelationKind::ContentDependency => "content_dependency",
            StructureRelationKind::DeploymentControl => "deployment_control",
            StructureRelationKind::RecoveryGroup => "recovery_group",
            StructureRelationKind::Observation => "observation",
        };
        edges.push(InfrastructureGraphEdgeDto {
            source_id: edge.from.0.clone(),
            target_id: edge.to.0.clone(),
            relation_kind: relation.into(),
            confidence: source
                .details
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("candidate")
                .into(),
            provenance_json: serde_json::json!({"role": edge.role, "fact": source.details})
                .to_string(),
        });
    }
    for node in graph.nodes {
        let domain = match node.kind {
            StructureNodeKind::StorageObject | StructureNodeKind::ObjectVersion => "storage",
            StructureNodeKind::EvidenceFact => "analysis",
            _ => "environment",
        };
        let kind = match node.kind {
            StructureNodeKind::PhysicalHost => "physical_host",
            StructureNodeKind::Workload => "workload",
            StructureNodeKind::Namespace => "namespace",
            StructureNodeKind::ObjectVersion => "object_version",
            StructureNodeKind::StorageObject => "storage_object",
            StructureNodeKind::SourceRevision => "source_revision",
            StructureNodeKind::Environment => "environment",
            StructureNodeKind::EvidenceFact => "evidence_fact",
            StructureNodeKind::Partition => "partition",
            StructureNodeKind::FileSystem => "filesystem",
        };
        let mut provenance = node.details.clone();
        if let Some(evidence) = node.details.get("evidence") {
            if let Some(object) = provenance.as_object_mut() {
                object.insert(
                    "dataSourceId".into(),
                    evidence["sourceRevision"]["sourceId"].clone(),
                );
                object.insert("fileId".into(), evidence["fileId"].clone());
                object.insert("parser".into(), evidence["parser"].clone());
            }
        }
        let version = (node.kind == StructureNodeKind::ObjectVersion).then(|| node.id.0.clone());
        nodes.push(InfrastructureGraphNodeDto {
            id: node.id.0,
            domain: domain.into(),
            kind: kind.into(),
            name: node.label,
            status: node
                .details
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("candidate")
                .into(),
            confidence: node
                .details
                .get("identityState")
                .and_then(serde_json::Value::as_str)
                .or_else(|| {
                    node.details
                        .get("status")
                        .and_then(serde_json::Value::as_str)
                })
                .unwrap_or("candidate")
                .into(),
            provenance_json: provenance.to_string(),
            version,
        });
    }
    Ok(())
}
