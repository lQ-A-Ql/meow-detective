use domain::structure::StructureGraph;

use super::{
    graph_builder::build_structure_graph,
    request::{StructureAnalysisError, StructureGraphRequest},
};

pub fn get_structure_graph(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    request: &StructureGraphRequest,
) -> Result<StructureGraph, StructureAnalysisError> {
    let transaction = conn
        .is_autocommit()
        .then(|| conn.unchecked_transaction())
        .transpose()
        .map_err(persistence_sqlite::DbError::from)?;
    let mut graph =
        build_structure_graph(conn, case_root, &request.case_id, &request.source_ids)?.graph;
    let facts = crate::structure_facts::get_structure_fact_graph(
        conn,
        case_root,
        &request.case_id,
        &request.source_ids,
    )?;
    let mut nodes = graph
        .nodes
        .into_iter()
        .map(|node| (node.id.clone(), node))
        .collect::<std::collections::BTreeMap<_, _>>();
    for node in facts.nodes {
        match nodes.entry(node.id.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(node);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                if let (Some(existing), Some(details)) = (
                    entry.get_mut().details.as_object_mut(),
                    node.details.as_object(),
                ) {
                    existing.extend(details.clone());
                }
            }
        }
    }
    graph.nodes = nodes.into_values().collect();
    graph.edges.extend(facts.edges);
    graph.edges.sort_by(|left, right| {
        (&left.from, &left.to, left.kind, &left.role).cmp(&(
            &right.from,
            &right.to,
            right.kind,
            &right.role,
        ))
    });
    graph.edges.dedup_by(|left, right| {
        left.from == right.from
            && left.to == right.to
            && left.kind == right.kind
            && left.role == right.role
    });
    graph
        .validate()
        .map_err(|error| StructureAnalysisError::Graph(error.to_string()))?;
    if let Some(transaction) = transaction {
        transaction
            .commit()
            .map_err(persistence_sqlite::DbError::from)?;
    }
    Ok(graph)
}
