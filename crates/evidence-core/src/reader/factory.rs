use std::io;
use std::sync::Arc;

use super::EvidenceReader;

/// Capabilities used by application services to choose a safe reader topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReaderCapabilities {
    pub independent_cursor: bool,
    pub max_concurrent_readers: usize,
    pub stable_snapshot: bool,
}

impl Default for ReaderCapabilities {
    fn default() -> Self {
        Self {
            independent_cursor: true,
            max_concurrent_readers: 1,
            stable_snapshot: true,
        }
    }
}

pub trait EvidenceReaderFactory: Send + Sync {
    fn open_reader(&self) -> io::Result<Box<dyn EvidenceReader>>;
    fn logical_size(&self) -> u64;
    fn preferred_read_granularity(&self) -> usize;
    fn capabilities(&self) -> ReaderCapabilities;
}

pub type SharedReaderFactory = Arc<dyn EvidenceReaderFactory>;
