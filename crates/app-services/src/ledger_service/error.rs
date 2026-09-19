#[derive(Debug, thiserror::Error)]
pub enum LedgerServiceError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("ledger entry or batch not found")]
    NotFound,
    #[error("invalid ledger request: {0}")]
    Invalid(String),
}

impl transport::ServiceErrorCategory for LedgerServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) => transport::ErrorCategory::Io,
            Self::NotFound | Self::Invalid(_) => transport::ErrorCategory::Validation,
        }
    }
}
