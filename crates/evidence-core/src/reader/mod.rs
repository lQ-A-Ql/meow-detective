use std::io::{Read, Seek};
use std::path::PathBuf;

mod admission;
mod factory;
mod metrics;
pub use metrics::EvidenceReadMetrics;
mod fixed_vhd;
pub use admission::{ReaderAdmission, ReaderAllocationScope, ReaderResourceHold};
mod partition_window;

pub use factory::{EvidenceReaderFactory, ReaderCapabilities, SharedReaderFactory};
pub use fixed_vhd::{fixed_vhd_payload_length, open_fixed_vhd_payload};
pub use partition_window::PartitionWindowReader;

/// Upper bound on the number of bytes read when inspecting file headers or magic values.
pub const MAGIC_HEADER_LIMIT: usize = 8 * 1024;

#[derive(Debug, Clone)]
pub struct ReaderInfo {
    pub path: PathBuf,
    pub size: u64,
    pub kind: String,
}

pub trait EvidenceReader: Read + Seek + Send {
    fn info(&self) -> &ReaderInfo;
    fn validate_read_scope(&self) -> std::io::Result<()> {
        Ok(())
    }

    fn read_metrics(&self) -> EvidenceReadMetrics {
        EvidenceReadMetrics::default()
    }

    /// Sequential scan callers own buffering and do not consume speculative bytes.
    fn configure_content_scan(&mut self) {}

    /// Preferred physical read size for callers that maintain an aligned
    /// metadata cache. Zero means that the reader has no explicit preference.
    fn preferred_read_granularity(&self) -> usize {
        0
    }
    fn reserve_work(
        &self,
        _bytes: u64,
    ) -> std::io::Result<Option<std::sync::Arc<dyn ReaderResourceHold>>> {
        Ok(None)
    }
}
