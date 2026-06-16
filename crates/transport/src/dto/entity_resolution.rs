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
}
