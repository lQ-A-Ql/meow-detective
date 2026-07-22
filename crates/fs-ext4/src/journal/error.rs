use std::io;

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("truncated JBD2 {context}: need {needed} bytes, have {available}")]
    Truncated {
        context: &'static str,
        needed: usize,
        available: usize,
    },

    #[error("invalid JBD2 data: {0}")]
    Invalid(String),

    #[error("unsupported JBD2 feature: {0}")]
    Unsupported(String),

    #[error("failed to read ext4 journal metadata: {0}")]
    Io(#[source] io::Error),
}

impl From<io::Error> for JournalError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub type JournalResult<T> = Result<T, JournalError>;

pub(crate) fn require_len(data: &[u8], needed: usize, context: &'static str) -> JournalResult<()> {
    if data.len() < needed {
        return Err(JournalError::Truncated {
            context,
            needed,
            available: data.len(),
        });
    }
    Ok(())
}
