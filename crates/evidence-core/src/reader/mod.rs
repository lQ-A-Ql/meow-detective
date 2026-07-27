use std::io::{Read, Seek};
use std::path::PathBuf;

mod partition_window;

pub use partition_window::PartitionWindowReader;

#[derive(Debug, Clone)]
pub struct ReaderInfo {
    pub path: PathBuf,
    pub size: u64,
    pub kind: String,
}

pub trait EvidenceReader: Read + Seek + Send {
    fn info(&self) -> &ReaderInfo;

    /// Preferred physical read size for callers that maintain an aligned
    /// metadata cache. Zero means that the reader has no explicit preference.
    fn preferred_read_granularity(&self) -> usize {
        0
    }
}
