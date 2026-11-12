use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use domain::{DataSourceId, DataSourceKind};
use evidence_core::{
    EvidenceReader, EvidenceReaderFactory, ReaderCapabilities, SharedReaderFactory,
};

use super::lease::SourcePublicationLease;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceIdentity {
    pub case_id: String,
    pub data_source_id: DataSourceId,
    pub revision: u64,
    pub source_kind: DataSourceKind,
    pub path: PathBuf,
}

pub struct SourceRuntime {
    identity: SourceIdentity,
    factory: SharedReaderFactory,
    generation: u64,
    lease: SourcePublicationLease,
}

impl SourceRuntime {
    pub fn new(identity: SourceIdentity, factory: SharedReaderFactory) -> Self {
        Self {
            lease: SourcePublicationLease::acquire(
                &identity.case_id,
                &identity.data_source_id.0,
                factory.clone(),
            ),
            identity,
            factory,
            generation: 0,
        }
    }

    pub fn identity(&self) -> &SourceIdentity {
        &self.identity
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn open_reader(&self) -> io::Result<Box<dyn EvidenceReader>> {
        self.factory().open_reader()
    }
    pub fn open_reader_with_cancel(
        &self,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> io::Result<Box<dyn EvidenceReader>> {
        self.factory().open_reader_with_cancel(cancel)
    }
    pub fn capabilities(&self) -> ReaderCapabilities {
        self.factory.capabilities()
    }
    pub fn logical_size(&self) -> u64 {
        self.factory.logical_size()
    }
    pub fn preferred_read_granularity(&self) -> usize {
        self.factory.preferred_read_granularity()
    }
    pub fn bump_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.lease = SourcePublicationLease::acquire(
            &self.identity.case_id,
            &self.identity.data_source_id.0,
            self.factory.clone(),
        );
    }
    pub fn factory(&self) -> SharedReaderFactory {
        Arc::new(LeasedFactory {
            inner: Arc::clone(&self.factory),
            lease: self.lease.clone(),
        })
    }
    pub fn source_path(&self) -> &Path {
        &self.identity.path
    }
    pub fn publication_lease(&self) -> SourcePublicationLease {
        self.lease.clone()
    }
}

struct LeasedFactory {
    inner: SharedReaderFactory,
    lease: SourcePublicationLease,
}

impl EvidenceReaderFactory for LeasedFactory {
    fn bind_reader_scope(&self, scope: Arc<dyn evidence_core::reader::ReaderAllocationScope>) {
        self.inner.bind_reader_scope(scope);
    }
    fn reader_memory_bound(&self) -> u64 {
        self.inner.reader_memory_bound()
    }
    fn reader_scope(&self) -> Option<Arc<dyn evidence_core::reader::ReaderAllocationScope>> {
        self.inner.reader_scope()
    }
    fn open_reader(&self) -> io::Result<Box<dyn EvidenceReader>> {
        self.lease.validate_epoch()?;
        let reader = self.inner.open_reader()?;
        self.lease.validate_epoch()?;
        Ok(reader)
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
        self.lease.validate()
    }
    fn metadata_identity(&self) -> Option<&str> {
        self.inner.metadata_identity()
    }
}
