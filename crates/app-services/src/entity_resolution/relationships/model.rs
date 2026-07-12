use serde::{Deserialize, Serialize};

/// The semantic relationship between two graph subjects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RelationshipType {
    CommunicatesWith,
    Owns,
    LoggedInto,
    Executed,
    Downloaded,
    Accessed,
}

impl RelationshipType {
    pub(super) fn as_db_str(&self) -> &'static str {
        match self {
            Self::CommunicatesWith => "communicates_with",
            Self::Owns => "owns",
            Self::LoggedInto => "logged_into",
            Self::Executed => "executed",
            Self::Downloaded => "downloaded",
            Self::Accessed => "accessed",
        }
    }
}

/// A relationship inferred from one or more graph edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRelationship {
    pub id: String,
    pub case_id: String,
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relationship_type: RelationshipType,
    pub confidence: f64,
    pub evidence_edge_ids: Vec<String>,
    pub created_at: String,
}
