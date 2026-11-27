use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{atomic::AtomicBool, Arc},
};

use evidence_core::{
    reader::{EvidenceReadMetrics, ReaderResourceHold},
    EvidenceReader, PartitionWindowReader, ReaderInfo,
};

use super::plan::check_cancel;

/// A zero-based verified range with cancellation checked at each operation.
pub struct VerifiedView<'a> {
    window: PartitionWindowReader,
    cancel: &'a AtomicBool,
}

impl<'a> VerifiedView<'a> {
    pub(super) fn new(window: PartitionWindowReader, cancel: &'a AtomicBool) -> Self {
        Self { window, cancel }
    }

    fn check_cancel(&self) -> io::Result<()> {
        // Interrupted would cause standard read_to_end/read_exact to retry an
        // already cancelled demand indefinitely. Keep cancellation terminal.
        check_cancel(self.cancel).map_err(io::Error::other)
    }
}

impl Read for VerifiedView<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.check_cancel()?;
        let position = self.window.stream_position()?;
        let count = self.window.read(output)?;
        if count == 0 && !output.is_empty() && position < self.window.info().size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "verified range ended before its declared length",
            ));
        }
        self.check_cancel()?;
        Ok(count)
    }
}

impl Seek for VerifiedView<'_> {
    fn seek(&mut self, target: SeekFrom) -> io::Result<u64> {
        self.check_cancel()?;
        self.window.seek(target)
    }
}

impl EvidenceReader for VerifiedView<'_> {
    fn info(&self) -> &ReaderInfo {
        self.window.info()
    }

    fn read_metrics(&self) -> EvidenceReadMetrics {
        self.window.read_metrics()
    }

    fn configure_content_scan(&mut self) {
        self.window.configure_content_scan();
    }

    fn reserve_work(&self, bytes: u64) -> io::Result<Option<Arc<dyn ReaderResourceHold>>> {
        self.check_cancel()?;
        self.window.reserve_work(bytes)
    }

    fn preferred_read_granularity(&self) -> usize {
        self.window.preferred_read_granularity()
    }
}
