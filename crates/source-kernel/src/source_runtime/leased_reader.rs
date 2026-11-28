use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::Arc,
};

use super::SourcePublicationLease;
use evidence_core::{
    reader::{EvidenceReadMetrics, ReaderResourceHold},
    EvidenceReader, ReaderInfo,
};

pub struct LeasedReader {
    inner: Box<dyn EvidenceReader>,
    lease: SourcePublicationLease,
}

impl LeasedReader {
    pub fn wrap(
        inner: Box<dyn EvidenceReader>,
        lease: SourcePublicationLease,
    ) -> io::Result<Box<dyn EvidenceReader>> {
        lease.validate_epoch()?;
        Ok(Box::new(Self { inner, lease }))
    }
}

impl Read for LeasedReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.lease.validate_epoch()?;
        let count = self.inner.read(bytes)?;
        if let Err(error) = self.lease.validate_epoch() {
            bytes[..count].fill(0);
            return Err(error);
        }
        Ok(count)
    }
}
impl Seek for LeasedReader {
    fn seek(&mut self, target: SeekFrom) -> io::Result<u64> {
        self.lease.validate_epoch()?;
        let position = self.inner.seek(target)?;
        self.lease.validate_epoch()?;
        Ok(position)
    }
}
impl EvidenceReader for LeasedReader {
    fn validate_read_scope(&self) -> io::Result<()> {
        self.lease.validate_epoch()?;
        self.inner.validate_read_scope()
    }
    fn info(&self) -> &ReaderInfo {
        self.inner.info()
    }
    fn read_metrics(&self) -> EvidenceReadMetrics {
        self.inner.read_metrics()
    }
    fn configure_content_scan(&mut self) {
        self.inner.configure_content_scan();
    }
    fn preferred_read_granularity(&self) -> usize {
        self.inner.preferred_read_granularity()
    }
    fn reserve_work(&self, bytes: u64) -> io::Result<Option<Arc<dyn ReaderResourceHold>>> {
        self.lease.validate_epoch()?;
        self.inner.reserve_work(bytes)
    }
}
