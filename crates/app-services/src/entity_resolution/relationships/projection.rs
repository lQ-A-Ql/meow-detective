use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use super::model::{EntityRelationship, RelationshipType};
use super::patterns::RelationshipRow;

pub(super) fn project_rows(
    case_id: &str,
    rows: Vec<RelationshipRow>,
    relationship_type: RelationshipType,
) -> Vec<EntityRelationship> {
    let now = chrono::Utc::now().to_rfc3339();
    rows.into_iter()
        .map(|(source_id, target_id, edges_a, edges_b)| {
            let evidence_edge_ids = collect_edge_ids(&edges_a, &edges_b);
            EntityRelationship {
                id: relationship_id(case_id, &source_id, &target_id, &relationship_type),
                case_id: case_id.to_string(),
                source_entity_id: source_id,
                target_entity_id: target_id,
                relationship_type: relationship_type.clone(),
                confidence: confidence(evidence_edge_ids.len()),
                evidence_edge_ids,
                created_at: now.clone(),
            }
        })
        .collect()
}

pub(super) fn deduplicate(relationships: Vec<EntityRelationship>) -> Vec<EntityRelationship> {
    type Key = (String, String, String, RelationshipType);
    let mut groups: BTreeMap<Key, (BTreeSet<String>, String)> = BTreeMap::new();
    for relationship in relationships {
        let key = (
            relationship.case_id,
            relationship.source_entity_id,
            relationship.target_entity_id,
            relationship.relationship_type,
        );
        let entry = groups
            .entry(key)
            .or_insert_with(|| (BTreeSet::new(), relationship.created_at));
        entry.0.extend(relationship.evidence_edge_ids);
    }

    groups
        .into_iter()
        .map(
            |((case_id, source_id, target_id, rel_type), (edges, created_at))| {
                let evidence_edge_ids: Vec<String> = edges.into_iter().collect();
                EntityRelationship {
                    id: relationship_id(&case_id, &source_id, &target_id, &rel_type),
                    case_id,
                    source_entity_id: source_id,
                    target_entity_id: target_id,
                    relationship_type: rel_type,
                    confidence: confidence(evidence_edge_ids.len()),
                    evidence_edge_ids,
                    created_at,
                }
            },
        )
        .collect()
}

fn collect_edge_ids(first: &str, second: &str) -> Vec<String> {
    first
        .split(',')
        .chain(second.split(','))
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn confidence(edge_count: usize) -> f64 {
    match edge_count {
        0 => 0.0,
        1 => 0.70,
        2 => 0.85,
        _ => 0.95,
    }
}

fn relationship_id(
    case_id: &str,
    source_id: &str,
    target_id: &str,
    relationship_type: &RelationshipType,
) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    case_id.hash(&mut hasher);
    source_id.hash(&mut hasher);
    target_id.hash(&mut hasher);
    relationship_type.as_db_str().hash(&mut hasher);
    format!("rel:{}:{:016x}", case_id, hasher.finish())
}
