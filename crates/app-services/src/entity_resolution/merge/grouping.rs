use std::collections::{BTreeMap, BTreeSet};

use persistence_sqlite::repositories::entity_repo;
use rusqlite::Connection;

use super::canonicalization::{
    canonicalize_entity, confidence, entity_type_from_tags, resolved_entity_id,
};
use super::model::ResolvedEntity;
use crate::entity_resolution::EntityResolutionError;

pub(super) fn merge_entities(
    conn: &Connection,
    case_id: &str,
) -> Result<Vec<ResolvedEntity>, EntityResolutionError> {
    let rows = entity_repo::query_entity_nodes(conn, case_id).map_err(EntityResolutionError::Db)?;
    let mut groups: BTreeMap<(String, String), Vec<(String, String)>> = BTreeMap::new();
    for (id, label, tags_json) in rows {
        let entity_type = entity_type_from_tags(&tags_json);
        let canonical = canonicalize_entity(&label, &entity_type);
        groups
            .entry((canonical, entity_type))
            .or_default()
            .push((id, label));
    }

    Ok(groups
        .into_iter()
        .map(|((canonical_value, entity_type), mut entities)| {
            entities.sort_by(|left, right| left.0.cmp(&right.0));
            build_resolved_entity(case_id, canonical_value, entity_type, entities)
        })
        .collect())
}

fn build_resolved_entity(
    case_id: &str,
    canonical_value: String,
    entity_type: String,
    entities: Vec<(String, String)>,
) -> ResolvedEntity {
    let source_entities: Vec<String> = entities.iter().map(|(id, _)| id.clone()).collect();
    let attributes: Vec<String> = entities
        .into_iter()
        .map(|(_, label)| label)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    ResolvedEntity {
        id: resolved_entity_id(case_id, &canonical_value, &entity_type),
        confidence: confidence(source_entities.len()),
        entity_type,
        canonical_value,
        source_entities,
        attributes,
    }
}
