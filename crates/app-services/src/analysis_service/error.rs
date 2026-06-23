use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalysisServiceError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("read error: {0}")]
    Read(String),
    #[error("artifact extraction error: {0}")]
    Extraction(String),
    #[error("{0} not found: {1}")]
    NotFound(&'static str, String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("{0}")]
    Other(String),
}

impl From<rusqlite::Error> for AnalysisServiceError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(persistence_sqlite::DbError::from(e))
    }
}
