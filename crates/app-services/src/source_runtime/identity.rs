use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use domain::{DataSourceId, DataSourceKind};
use evidence_core::{EvidenceReader, ReaderCapabilities, SharedReaderFactory};

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
}

impl SourceRuntime {
    pub fn new(identity: SourceIdentity, factory: SharedReaderFactory) -> Self {
        Self {
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
        self.factory.open_reader()
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
    }
    pub fn factory(&self) -> SharedReaderFactory {
        Arc::clone(&self.factory)
    }
    pub fn source_path(&self) -> &Path {
        &self.identity.path
    }
}
