use super::{ContentVerifiedReader, SourceContent, CONTENT_CHUNK_BYTES};
use evidence_core::{
    EvidenceReader, EvidenceReaderFactory, ReaderCapabilities, SharedReaderFactory,
};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub struct VerifiedFactory {
    inner: SharedReaderFactory,
    content: Arc<SourceContent>,
    _manifest: Arc<dyn evidence_core::reader::ReaderResourceHold>,
    failed_reads: Arc<AtomicBool>,
}

impl VerifiedFactory {
    pub fn wrap(
        inner: SharedReaderFactory,
        content: Arc<SourceContent>,
        manifest: Arc<dyn evidence_core::reader::ReaderResourceHold>,
        failed_reads: Arc<AtomicBool>,
    ) -> SharedReaderFactory {
        Arc::new(Self {
            inner,
            content,
            _manifest: manifest,
            failed_reads,
        })
    }
}

impl EvidenceReaderFactory for VerifiedFactory {
    fn bind_reader_scope(&self, scope: Arc<dyn evidence_core::reader::ReaderAllocationScope>) {
        self.inner.bind_reader_scope(scope);
    }
    fn reader_scope(&self) -> Option<Arc<dyn evidence_core::reader::ReaderAllocationScope>> {
        self.inner.reader_scope()
    }
    fn reader_memory_bound(&self) -> u64 {
        self.inner.reader_memory_bound() + CONTENT_CHUNK_BYTES as u64
    }
    fn open_reader(&self) -> io::Result<Box<dyn EvidenceReader>> {
        self.inner.validate_snapshot()?;
        let source = self.inner.open_reader()?;
        let memory = source.reserve_work(CONTENT_CHUNK_BYTES as u64)?;
        Ok(Box::new(HeldReader {
            reader: ContentVerifiedReader::new(source, Arc::clone(&self.content))?,
            _memory: memory,
            _manifest: self._manifest.clone(),
            failed_reads: self.failed_reads.clone(),
        }))
    }
    fn logical_size(&self) -> u64 {
        self.inner.logical_size()
    }
    fn preferred_read_granularity(&self) -> usize {
        self.inner.preferred_read_granularity()
    }
    fn capabilities(&self) -> ReaderCapabilities {
        self.inner.capabilities()
    }
    fn validate_snapshot(&self) -> io::Result<()> {
        if self.failed_reads.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "collection verified read failed",
            ));
        }
        self.inner.validate_snapshot()
    }
    fn metadata_identity(&self) -> Option<&str> {
        self.inner.metadata_identity()
    }
}

struct HeldReader {
    reader: ContentVerifiedReader,
    _memory: Option<Arc<dyn evidence_core::reader::ReaderResourceHold>>,
    _manifest: Arc<dyn evidence_core::reader::ReaderResourceHold>,
    failed_reads: Arc<AtomicBool>,
}
impl io::Read for HeldReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let result = self.reader.read(bytes);
        if result.is_err() {
            self.failed_reads.store(true, Ordering::Release);
        }
        result
    }
}
impl io::Seek for HeldReader {
    fn seek(&mut self, position: io::SeekFrom) -> io::Result<u64> {
        self.reader.seek(position)
    }
}
impl EvidenceReader for HeldReader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        self.reader.info()
    }
    fn preferred_read_granularity(&self) -> usize {
        self.reader.preferred_read_granularity()
    }
    fn reserve_work(
        &self,
        bytes: u64,
    ) -> io::Result<Option<Arc<dyn evidence_core::reader::ReaderResourceHold>>> {
        self.reader.reserve_work(bytes)
    }
}
