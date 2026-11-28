mod backend;
mod collect;
mod host;
mod journal;
mod kubernetes;
mod model;
mod pve;

use domain::{structure::StructureGraph, CaseId};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;

use crate::structure_analysis::StructureAnalysisError;

pub fn get_structure_fact_graph(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    source_ids: &[String],
) -> Result<StructureGraph, StructureAnalysisError> {
    let transaction = conn
        .is_autocommit()
        .then(|| conn.unchecked_transaction())
        .transpose()
        .map_err(persistence_sqlite::DbError::from)?;
    let mut projection = model::FactProjection::default();
    let sources = DataSourceRepo::new(conn).find_by_case(case_id)?;
    for requested in source_ids {
        if !sources.iter().any(|source| source.id.0 == *requested) {
            return Err(StructureAnalysisError::MissingSource(requested.clone()));
        }
    }
    for source in sources {
        if !source_ids.is_empty() && !source_ids.contains(&source.id.0) {
            continue;
        }
        let storage = DataSourceRepo::new(conn).find_storage(&source.id)?;
        let Some(storage) = storage.filter(|storage| {
            storage.platform == "linux"
                && matches!(storage.import_state.as_str(), "ready" | "ready_metadata")
        }) else {
            continue;
        };
        collect::collect_source(
            conn,
            case_root,
            case_id,
            &source,
            &storage.import_state,
            &mut projection,
        )?;
    }
    let graph = projection.finish()?;
    if let Some(transaction) = transaction {
        transaction
            .commit()
            .map_err(persistence_sqlite::DbError::from)?;
    }
    Ok(graph)
}

#[cfg(test)]
#[path = "../tests/unit/structure_facts.rs"]
mod tests;
