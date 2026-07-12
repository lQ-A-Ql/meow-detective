mod canonicalization;
mod grouping;
mod model;
mod persistence;

pub use model::ResolvedEntity;

use rusqlite::Connection;

use super::EntityResolutionError;

/// Canonicalizes and deduplicates entity graph nodes.
pub struct EntityMergeEngine;

impl EntityMergeEngine {
    pub fn canonicalize_entity(value: &str, entity_type: &str) -> String {
        canonicalization::canonicalize_entity(value, entity_type)
    }

    pub fn merge_entities(
        conn: &Connection,
        case_id: &str,
    ) -> Result<Vec<ResolvedEntity>, EntityResolutionError> {
        grouping::merge_entities(conn, case_id)
    }

    pub fn deduplicate_entity_nodes(
        conn: &Connection,
        case_id: &str,
    ) -> Result<u64, EntityResolutionError> {
        persistence::deduplicate_entity_nodes(conn, case_id)
    }
}
