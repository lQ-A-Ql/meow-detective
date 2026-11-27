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
    Ok(build_structure_graph(conn, case_root, &request.case_id, &request.source_ids)?.graph)
}
