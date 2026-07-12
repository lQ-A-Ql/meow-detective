use rusqlite::Connection;

use super::model::{EntityRelationship, RelationshipType};
use super::{patterns, projection};
use crate::entity_resolution::EntityResolutionError;

pub(super) fn infer_relationships(
    conn: &Connection,
    case_id: &str,
) -> Result<Vec<EntityRelationship>, EntityResolutionError> {
    let mut relationships = Vec::new();
    append_pattern(
        &mut relationships,
        case_id,
        patterns::communicates_with(conn, case_id)?,
        RelationshipType::CommunicatesWith,
    );
    append_pattern(
        &mut relationships,
        case_id,
        patterns::ownership(conn, case_id)?,
        RelationshipType::Owns,
    );
    append_pattern(
        &mut relationships,
        case_id,
        patterns::logged_into(conn, case_id)?,
        RelationshipType::LoggedInto,
    );
    append_pattern(
        &mut relationships,
        case_id,
        patterns::executed(conn, case_id)?,
        RelationshipType::Executed,
    );
    Ok(projection::deduplicate(relationships))
}

fn append_pattern(
    relationships: &mut Vec<EntityRelationship>,
    case_id: &str,
    rows: Vec<patterns::RelationshipRow>,
    relationship_type: RelationshipType,
) {
    relationships.extend(projection::project_rows(case_id, rows, relationship_type));
}
