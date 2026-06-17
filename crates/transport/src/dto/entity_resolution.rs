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
mod tests {
    use super::*;

    #[test]
    fn resolved_entity_serializes_camel_case() {
        let entity = ResolvedEntityDto {
            id: "resolved:case-1:abc123".to_string(),
            entity_type: "person".to_string(),
            canonical_value: "alice@example.com".to_string(),
            source_entities: vec![
                "entity:case-1:a1".to_string(),
                "entity:case-1:a2".to_string(),
            ],
            source_count: 2,
            confidence: 0.85,
            attributes: vec![
                "Alice@Example.COM".to_string(),
                "alice@example.com".to_string(),
            ],
        };

        let json = serde_json::to_value(entity).unwrap();
        assert_eq!(json["id"], "resolved:case-1:abc123");
        assert_eq!(json["entityType"], "person");
        assert_eq!(json["canonicalValue"], "alice@example.com");
        assert_eq!(json["sourceCount"], 2);
        assert_eq!(json["confidence"], 0.85);
        assert_eq!(json["sourceEntities"][0], "entity:case-1:a1");
        assert_eq!(json["attributes"][0], "Alice@Example.COM");
        // Confirm snake_case keys are absent
        assert!(json.get("entity_type").is_none());
        assert!(json.get("canonical_value").is_none());
    }

    #[test]
    fn merge_result_serializes_camel_case() {
        let result = EntityMergeResultDto {
            resolved_count: 2,
            merged_count: 3,
            resolved: vec![ResolvedEntityDto {
                id: "resolved:case-1:abc".to_string(),
                entity_type: "person".to_string(),
                canonical_value: "alice@example.com".to_string(),
                source_entities: vec!["entity:case-1:a1".to_string()],
                source_count: 1,
                confidence: 0.70,
                attributes: vec![],
            }],
        };

        let json = serde_json::to_value(result).unwrap();
        assert_eq!(json["resolvedCount"], 2);
        assert_eq!(json["mergedCount"], 3);
        assert_eq!(json["resolved"][0]["entityType"], "person");
        // Confirm snake_case keys are absent
        assert!(json.get("resolved_count").is_none());
    }

    #[test]
    fn relationship_type_serializes_camel_case() {
        let t = EntityRelationshipTypeDto::CommunicatesWith;
        let json = serde_json::to_value(t).unwrap();
        // camelCase: CommunicatesWith → communicatesWith
        assert_eq!(json, "communicatesWith");

        let t = EntityRelationshipTypeDto::LoggedInto;
        let json = serde_json::to_value(t).unwrap();
        assert_eq!(json, "loggedInto");
    }

    #[test]
    fn entity_relationship_serializes_camel_case() {
        let rel = EntityRelationshipDto {
            id: "rel:case-1:abc".to_string(),
            case_id: "case-1".to_string(),
            source_entity_id: "entity-alice".to_string(),
            target_entity_id: "entity-bob".to_string(),
            relationship_type: EntityRelationshipTypeDto::CommunicatesWith,
            confidence: 0.85,
            evidence_edge_ids: vec!["edge-1".to_string(), "edge-2".to_string()],
            created_at: "2025-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_value(rel).unwrap();
        assert_eq!(json["id"], "rel:case-1:abc");
        assert_eq!(json["caseId"], "case-1");
        assert_eq!(json["sourceEntityId"], "entity-alice");
        assert_eq!(json["targetEntityId"], "entity-bob");
        assert_eq!(json["relationshipType"], "communicatesWith");
        assert_eq!(json["confidence"], 0.85);
        assert_eq!(json["evidenceEdgeIds"][0], "edge-1");
        assert_eq!(json["createdAt"], "2025-01-01T00:00:00Z");
        // Confirm snake_case keys are absent
        assert!(json.get("source_entity_id").is_none());
        assert!(json.get("relationship_type").is_none());
    }

    #[test]
    fn relationship_result_serializes_camel_case() {
        let result = EntityRelationshipResultDto {
            relationship_count: 1,
            relationships: vec![EntityRelationshipDto {
                id: "rel:case-1:abc".to_string(),
                case_id: "case-1".to_string(),
                source_entity_id: "entity-alice".to_string(),
                target_entity_id: "entity-bob".to_string(),
                relationship_type: EntityRelationshipTypeDto::CommunicatesWith,
                confidence: 0.85,
                evidence_edge_ids: vec!["edge-1".to_string()],
                created_at: "2025-01-01T00:00:00Z".to_string(),
            }],
        };

        let json = serde_json::to_value(result).unwrap();
        assert_eq!(json["relationshipCount"], 1);
        assert_eq!(
            json["relationships"][0]["relationshipType"],
            "communicatesWith"
        );
        assert!(json.get("relationship_count").is_none());
    }
}
