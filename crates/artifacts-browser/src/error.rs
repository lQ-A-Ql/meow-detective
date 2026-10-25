#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error("browser database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("browser input could not be read")]
    Io(#[from] std::io::Error),
    #[error("browser input is invalid: {0}")]
    Parse(String),
    #[error("browser schema is unsupported: {0}")]
    Unsupported(&'static str),
    #[error("browser input exceeds the byte limit")]
    InputLimit,
    #[error("browser allocation failed")]
    Allocation,
    #[error("browser parsing cancelled")]
    Cancelled,
}

impl From<String> for BrowserError {
    fn from(message: String) -> Self {
        Self::Parse(message)
    }
}

pub type Result<T> = std::result::Result<T, BrowserError>;
