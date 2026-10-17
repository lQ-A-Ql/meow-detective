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
    fn bind_reader_scope(&self, _scope: Arc<dyn super::admission::ReaderAllocationScope>) {}
    fn reader_memory_bound(&self) -> u64 {
        0
    }
    fn reader_scope(&self) -> Option<Arc<dyn super::admission::ReaderAllocationScope>> {
        None
    }
    fn open_reader(&self) -> io::Result<Box<dyn EvidenceReader>>;
    fn open_reader_with_cancel(
        &self,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> io::Result<Box<dyn EvidenceReader>> {
        loop {
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Reader admission cancelled",
                ));
            }
            match self.open_reader() {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(10))
                }
                result => return result,
            }
        }
    }
    fn logical_size(&self) -> u64;
    fn preferred_read_granularity(&self) -> usize;
    fn capabilities(&self) -> ReaderCapabilities;
    /// Recheck the physical members before publishing derived results. This is
    /// metadata identity validation, not a cryptographic evidence digest.
    fn validate_snapshot(&self) -> io::Result<()> {
        Ok(())
    }
    /// Versioned physical-member metadata identity, never a content digest.
    fn metadata_identity(&self) -> Option<&str> {
        None
    }
}

pub type SharedReaderFactory = Arc<dyn EvidenceReaderFactory>;
