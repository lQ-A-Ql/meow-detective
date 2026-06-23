pub mod cross_case;
pub mod merge;
pub mod relationships;

pub use cross_case::{CrossCaseEntityMatcher, CrossCaseMatch, MatchStrategy};
pub use merge::{EntityMergeEngine, ResolvedEntity};
pub use relationships::{EntityRelationship, EntityRelationshipEngine, RelationshipType};

use thiserror::Error;

/// Unified error type for entity resolution operations.
#[derive(Debug, Error)]
pub enum EntityResolutionError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("{0}")]
    Other(String),
}

impl From<rusqlite::Error> for EntityResolutionError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(persistence_sqlite::DbError::from(e))
    }
}
