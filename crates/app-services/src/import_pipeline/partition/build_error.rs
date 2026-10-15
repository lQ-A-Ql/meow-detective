use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum PartitionBuildError {
    #[error("No filesystem candidate matches partition {0}")]
    MissingCandidate(usize),
    #[error("Filesystem candidate is not available for enumeration")]
    Unavailable,
    #[error("Filesystem reader initialization failed")]
    Reader(#[source] std::io::Error),
}
