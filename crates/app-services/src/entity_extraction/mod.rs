//! Entity extraction from artifacts.
//!
//! The facade keeps normalization, indexing, lookup, and graph extraction
//! stable while the implementation is split by responsibility.

mod error;
mod index;
mod normalization;
mod persistence;
mod scan;

pub use error::EntityExtractionError;
pub use index::{index_entities, lookup_entity};
pub use normalization::{hash_entity_value, normalize_entity_value};

use rusqlite::Connection;

/// Extract entities from case artifacts and persist their graph projection.
pub fn extract_entities_from_artifacts(
    conn: &Connection,
    case_id: &str,
) -> Result<u64, EntityExtractionError> {
    let artifact_ids = scan::artifact_ids_for_case(conn, case_id)?;
    if !artifact_ids.is_empty() {
        let indexed = index::entity_map_from_index(conn, &artifact_ids)?;
        if !indexed.is_empty() {
            return persistence::persist_entity_graph(conn, case_id, indexed);
        }
    }

    let scanned = scan::scan_artifacts(conn, case_id)?;
    if scanned.is_empty() {
        return Ok(0);
    }
    persistence::persist_entity_graph(conn, case_id, scanned)
}
