/// A resolved entity after canonicalization and grouping.
#[derive(Debug, Clone)]
pub struct ResolvedEntity {
    pub id: String,
    pub entity_type: String,
    pub canonical_value: String,
    pub source_entities: Vec<String>,
    pub confidence: f64,
    pub attributes: Vec<String>,
}
