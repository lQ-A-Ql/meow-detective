#[derive(Debug, thiserror::Error)]
pub enum GovernanceError {
    #[error("database operation failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("failed to parse governance fact source: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("internal governance error: {0}")]
    Internal(String),
}

impl From<persistence_sqlite::DbError> for GovernanceError {
    fn from(err: persistence_sqlite::DbError) -> Self {
        match err {
            persistence_sqlite::DbError::Sqlite(e) => GovernanceError::Database(e),
            _ => GovernanceError::Internal(err.to_string()),
        }
    }
}
