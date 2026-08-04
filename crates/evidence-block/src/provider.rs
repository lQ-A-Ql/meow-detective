use std::path::Path;
use std::sync::Arc;

use crate::e01::E01BlockProvider;
use crate::raw::RawBlockProvider;
use crate::BlockDeviceError;

pub trait BlockProvider: Send + Sync {
    fn len(&self) -> u64;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn read_exact_at(&self, offset: u64, buffer: &mut [u8]) -> Result<(), BlockDeviceError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceImageKind {
    E01,
    Raw,
}

pub fn open_block_provider(
    path: &Path,
    kind: EvidenceImageKind,
) -> Result<Arc<dyn BlockProvider>, BlockDeviceError> {
    match kind {
        EvidenceImageKind::E01 => Ok(Arc::new(E01BlockProvider::open(path)?)),
        EvidenceImageKind::Raw => Ok(Arc::new(RawBlockProvider::open(path)?)),
    }
}
