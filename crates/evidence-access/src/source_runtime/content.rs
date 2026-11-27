use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use domain::DataSourceKind;

mod factory;
mod reader;
mod scan;
mod telemetry;
pub use factory::VerifiedFactory;
pub(crate) use reader::ContentVerifiedReader;

pub const CONTENT_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("source content access failed: {0}")]
    Io(std::io::Error),
    #[error("source content identity is invalid")]
    InvalidIdentity,
    #[error("source content changed since planning")]
    Changed,
    #[error("source content verification cancelled")]
    Cancelled,
    #[error("content scan worker count must be at least four")]
    InvalidWorkerCount,
    #[error("source does not support the requested independent content readers")]
    ParallelReaderUnsupported,
    #[error("content scan worker terminated unexpectedly")]
    WorkerPanicked,
}

impl From<std::io::Error> for ContentError {
    fn from(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::Interrupted {
            Self::Cancelled
        } else {
            Self::Io(error)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceContent {
    pub source_path: String,
    pub source_kind: DataSourceKind,
    pub logical_size: u64,
    pub logical_sha256: String,
    pub chunk_bytes: u32,
    pub chunk_sha256: Vec<String>,
}

impl SourceContent {
    pub(crate) fn estimated_bytes(size: u64) -> Result<u64, ContentError> {
        size.div_ceil(CONTENT_CHUNK_BYTES as u64)
            .checked_mul((64 + std::mem::size_of::<String>()) as u64)
            .and_then(|bytes| bytes.checked_mul(2))
            .and_then(|bytes| bytes.checked_add(4096))
            .ok_or(ContentError::InvalidIdentity)
    }

    pub fn validate(&self) -> Result<(), ContentError> {
        if self.chunk_bytes != CONTENT_CHUNK_BYTES as u32
            || self.logical_size.div_ceil(CONTENT_CHUNK_BYTES as u64)
                != self.chunk_sha256.len() as u64
            || !valid_digest(&self.logical_sha256)
            || !self.chunk_sha256.iter().all(|digest| valid_digest(digest))
        {
            return Err(ContentError::InvalidIdentity);
        }
        Ok(())
    }

    pub fn verify(&self, cancel: &AtomicBool) -> Result<(), ContentError> {
        self.validate()?;
        check_cancel(cancel)?;
        let factory = super::open_factory_with_cancel(
            Path::new(&self.source_path),
            &self.source_kind,
            cancel,
        )?;
        self.verify_factory(&factory, cancel)
    }

    pub(crate) fn verify_factory(
        &self,
        factory: &evidence_core::SharedReaderFactory,
        cancel: &AtomicBool,
    ) -> Result<(), ContentError> {
        self.validate()?;
        scan::scan(factory, Some(self), cancel, scan::requested_workers()?)?;
        Ok(())
    }
}

pub fn capture(
    path: &Path,
    kind: &DataSourceKind,
    cancel: &AtomicBool,
) -> Result<SourceContent, ContentError> {
    check_cancel(cancel)?;
    let factory = super::open_factory_with_cancel(path, kind, cancel)?;
    capture_factory(path, kind, &factory, cancel)
}

pub(crate) fn capture_factory(
    path: &Path,
    kind: &DataSourceKind,
    factory: &evidence_core::SharedReaderFactory,
    cancel: &AtomicBool,
) -> Result<SourceContent, ContentError> {
    let result = scan::scan(factory, None, cancel, scan::requested_workers()?)?;
    Ok(SourceContent {
        source_path: path.canonicalize()?.to_string_lossy().into_owned(),
        source_kind: kind.clone(),
        logical_size: result.size,
        logical_sha256: result.digest,
        chunk_bytes: CONTENT_CHUNK_BYTES as u32,
        chunk_sha256: result.chunks,
    })
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), ContentError> {
    if cancel.load(Ordering::Acquire) {
        return Err(ContentError::Cancelled);
    }
    Ok(())
}

fn valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
#[path = "../../tests/unit/source_content.rs"]
mod tests;
