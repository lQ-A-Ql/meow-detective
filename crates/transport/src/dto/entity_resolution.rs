use serde::{Deserialize, Serialize};

/// A resolved entity after canonicalization and merge.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedEntityDto {
    pub id: String,
    pub entity_type: String,
    pub canonical_value: String,
    pub source_entities: Vec<String>,
    pub source_count: u32,
    pub confidence: f64,
    pub attributes: Vec<String>,
}

/// Result of a deduplication merge pass.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntityMergeResultDto {
    pub resolved_count: u32,
    pub merged_count: u64,
    pub resolved: Vec<ResolvedEntityDto>,
}

/// The type of an inferred entity-to-entity relationship.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EntityRelationshipTypeDto {
    CommunicatesWith,
    Owns,
    LoggedInto,
    Executed,
    Downloaded,
    Accessed,
}

/// An inferred relationship between two entities, discovered from graph edge
/// patterns during entity resolution Phase 2.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntityRelationshipDto {
    pub id: String,
    pub case_id: String,
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relationship_type: EntityRelationshipTypeDto,
    pub confidence: f64,
    pub evidence_edge_ids: Vec<String>,
    pub created_at: String,
}

/// Result payload for entity relationship inference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntityRelationshipResultDto {
    pub relationship_count: u32,
    pub relationships: Vec<EntityRelationshipDto>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/entity_resolution.rs"]
mod tests;
