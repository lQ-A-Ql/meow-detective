use std::{
    io::SeekFrom,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use domain::DataSourceKind;
use evidence_core::EvidenceReader;
use sha2::{Digest, Sha256};

mod factory;
mod reader;
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
    pub fn estimated_bytes(size: u64) -> Result<u64, ContentError> {
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
        check_cancel(cancel)?;
        let mut reader = factory.open_reader_with_cancel(cancel)?;
        if reader.info().size != self.logical_size {
            return Err(ContentError::Changed);
        }
        let _memory = reader.reserve_work(CONTENT_CHUNK_BYTES as u64)?;
        let mut buffer = vec![0; CONTENT_CHUNK_BYTES];
        let mut digest = Sha256::new();
        let mut position = 0u64;
        for expected in &self.chunk_sha256 {
            check_cancel(cancel)?;
            let count = (self.logical_size - position).min(CONTENT_CHUNK_BYTES as u64) as usize;
            reader.read_exact(&mut buffer[..count])?;
            if hex::encode(Sha256::digest(&buffer[..count])) != *expected {
                return Err(ContentError::Changed);
            }
            digest.update(&buffer[..count]);
            position += count as u64;
        }
        if hex::encode(digest.finalize()) != self.logical_sha256
            || reader.read(&mut buffer[..1])? != 0
        {
            return Err(ContentError::Changed);
        }
        check_cancel(cancel)?;
        factory.validate_snapshot()?;
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
    let mut reader = factory.open_reader_with_cancel(cancel)?;
    let result = capture_reader(path, kind, &mut *reader, cancel)?;
    factory.validate_snapshot()?;
    Ok(result)
}

pub(crate) fn capture_reader(
    path: &Path,
    kind: &DataSourceKind,
    reader: &mut dyn EvidenceReader,
    cancel: &AtomicBool,
) -> Result<SourceContent, ContentError> {
    check_cancel(cancel)?;
    let size = reader.info().size;
    let _permit = source_kernel::resource_budget::global_pool()
        .acquire(
            source_kernel::resource_budget::Resources {
                bytes: SourceContent::estimated_bytes(size)? + CONTENT_CHUNK_BYTES as u64,
                ..Default::default()
            },
            Some(cancel),
        )
        .map_err(source_kernel::resource_budget::ResourceError::into_io)?;
    let mut digest = Sha256::new();
    let mut chunks = Vec::new();
    let mut position = 0u64;
    let mut buffer = vec![0; CONTENT_CHUNK_BYTES];
    reader.seek(SeekFrom::Start(0))?;
    while position < size {
        check_cancel(cancel)?;
        let count = (size - position).min(CONTENT_CHUNK_BYTES as u64) as usize;
        reader.read_exact(&mut buffer[..count])?;
        digest.update(&buffer[..count]);
        chunks.push(hex::encode(Sha256::digest(&buffer[..count])));
        position += count as u64;
    }
    if reader.read(&mut buffer[..1])? != 0 || reader.info().size != size {
        return Err(ContentError::InvalidIdentity);
    }
    check_cancel(cancel)?;
    Ok(SourceContent {
        source_path: path.canonicalize()?.to_string_lossy().into_owned(),
        source_kind: kind.clone(),
        logical_size: size,
        logical_sha256: hex::encode(digest.finalize()),
        chunk_bytes: CONTENT_CHUNK_BYTES as u32,
        chunk_sha256: chunks,
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
