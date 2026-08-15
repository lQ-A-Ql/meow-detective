use rusqlite::Connection;

use super::super::EntityResolutionError;
use super::model::EntityRelationship;
use super::{inference, persistence};

/// Infers and persists semantic relationships between entities.
pub struct EntityRelationshipEngine;

impl EntityRelationshipEngine {
    pub fn infer_relationships(
        conn: &Connection,
        case_id: &str,
    ) -> Result<Vec<EntityRelationship>, EntityResolutionError> {
        inference::infer_relationships(conn, case_id)
    }

    pub fn persist_relationships(
        conn: &Connection,
        case_id: &str,
        relationships: &[EntityRelationship],
    ) -> Result<u64, EntityResolutionError> {
        persistence::persist_relationships(conn, case_id, relationships)
    }
}
