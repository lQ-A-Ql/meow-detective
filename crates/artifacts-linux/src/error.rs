#[derive(Debug, thiserror::Error)]
pub enum LinuxArtifactError {
    #[error("parse error in {parser}: {message}")]
    ParseError {
        parser: &'static str,
        message: String,
    },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
