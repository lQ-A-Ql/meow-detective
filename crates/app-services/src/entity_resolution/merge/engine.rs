use rusqlite::Connection;

use super::super::EntityResolutionError;
use super::model::ResolvedEntity;
use super::{canonicalization, grouping, persistence};

/// Canonicalizes and deduplicates entity graph nodes.
pub struct EntityMergeEngine;

impl EntityMergeEngine {
    pub fn canonicalize_entity(value: &str, entity_type: &str) -> String {
        canonicalization::canonicalize_entity(value, entity_type)
    }

    pub fn entity_type_from_tags(tags: &[String]) -> String {
        tags.iter()
            .find(|tag| tag.as_str() != "entity")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string())
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
