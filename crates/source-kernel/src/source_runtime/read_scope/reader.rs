use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::Arc,
};

use evidence_core::{
    reader::{EvidenceReadMetrics, ReaderResourceHold},
    EvidenceReader, ReaderInfo,
};

use super::SourceReadScope;

pub struct ScopedEvidenceReader {
    inner: Box<dyn EvidenceReader>,
    scope: SourceReadScope,
}

impl ScopedEvidenceReader {
    pub(super) fn new(inner: Box<dyn EvidenceReader>, scope: SourceReadScope) -> Self {
        Self { inner, scope }
    }
}

impl Read for ScopedEvidenceReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.scope.validate()?;
        let count = self.inner.read(bytes)?;
        if let Err(error) = self.scope.validate() {
            bytes[..count].fill(0);
            return Err(error);
        }
        Ok(count)
    }
}

impl Seek for ScopedEvidenceReader {
    fn seek(&mut self, target: SeekFrom) -> io::Result<u64> {
        self.scope.validate()?;
        let position = self.inner.seek(target)?;
        self.scope.validate()?;
        Ok(position)
    }
}

impl EvidenceReader for ScopedEvidenceReader {
    fn validate_read_scope(&self) -> io::Result<()> {
        self.scope.validate()?;
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
    fn reserve_work(&self, bytes: u64) -> io::Result<Option<Arc<dyn ReaderResourceHold>>> {
        self.scope.validate()?;
        self.inner.reserve_work(bytes)
    }
    fn preferred_read_granularity(&self) -> usize {
        self.inner.preferred_read_granularity()
    }
}
