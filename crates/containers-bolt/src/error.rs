#[derive(Debug, thiserror::Error)]
pub enum BoltError {
    #[error("Bolt input could not be read")]
    Io(#[from] std::io::Error),
    #[error("Bolt metadata is invalid")]
    Backend(#[from] bbolt_rs::Error),
    #[error("Bolt read options are invalid")]
    InvalidOptions,
    #[error("Bolt input exceeds the byte limit")]
    FileLimit,
    #[error("Bolt traversal exceeds the entry limit")]
    EntryLimit,
    #[error("Bolt traversal exceeds the bucket depth limit")]
    DepthLimit,
    #[error("Bolt result exceeds the byte limit")]
    OutputLimit,
    #[error("Bolt bucket is invalid")]
    InvalidBucket,
}
