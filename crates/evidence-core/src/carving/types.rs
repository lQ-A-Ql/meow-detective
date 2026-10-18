use std::sync::atomic::AtomicBool;

#[derive(Debug, thiserror::Error)]
pub enum CarveError {
    #[error("Invalid carving source view or range")]
    InvalidRange,
    #[error("Carving resource limit exceeded")]
    Limit,
    #[error("Carving cancelled")]
    Cancelled,
    #[error("Carving evidence read failed")]
    Io(#[from] std::io::Error),
    #[error("Carving result consumer stopped")]
    ConsumerStopped,
}

#[derive(Debug, Clone)]
pub struct CarveScope {
    pub source_id: String,
    pub revision: String,
    pub view: String,
    pub source_length: u64,
    pub offset: u64,
    pub length: u64,
}
impl CarveScope {
    pub fn end(&self) -> Result<u64, CarveError> {
        let end = self
            .offset
            .checked_add(self.length)
            .ok_or(CarveError::InvalidRange)?;
        if self.source_id.is_empty()
            || self.source_id.len() > 256
            || self.revision.is_empty()
            || self.revision.len() > 256
            || self.view != "logicalDisk"
            || self.length == 0
            || end > self.source_length
        {
            return Err(CarveError::InvalidRange);
        }
        Ok(end)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CarveLimits {
    pub window_bytes: usize,
    pub max_candidates: u32,
    pub max_object_bytes: u64,
    pub max_chunk_bytes: u32,
    pub max_chunks: u32,
    pub validation_bytes: u64,
    pub inflated_bytes: u64,
}
impl Default for CarveLimits {
    fn default() -> Self {
        Self {
            window_bytes: 1024 * 1024,
            max_candidates: 4096,
            max_object_bytes: 64 * 1024 * 1024,
            max_chunk_bytes: 16 * 1024 * 1024,
            max_chunks: 8192,
            validation_bytes: 512 * 1024 * 1024,
            inflated_bytes: 512 * 1024 * 1024,
        }
    }
}
impl CarveLimits {
    pub fn validate(self) -> Result<(), CarveError> {
        if !(8..=8 * 1024 * 1024).contains(&self.window_bytes)
            || !(1..=4096).contains(&self.max_candidates)
            || !(57..=128 * 1024 * 1024).contains(&self.max_object_bytes)
            || !(13..=16 * 1024 * 1024).contains(&self.max_chunk_bytes)
            || !(3..=65536).contains(&self.max_chunks)
            || self.validation_bytes > 512 * 1024 * 1024
            || self.inflated_bytes > 512 * 1024 * 1024
        {
            return Err(CarveError::Limit);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarveStatus {
    Complete,
    Partial,
    Rejected,
}

#[derive(Debug, Clone)]
pub struct CarveCandidate {
    pub source_id: String,
    pub revision: String,
    pub view: String,
    pub offset: u64,
    pub length: Option<u64>,
    pub sha256: Option<String>,
    pub status: CarveStatus,
    pub reason: &'static str,
    pub validator_version: &'static str,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ScanReport {
    pub scanned_bytes: u64,
    pub candidates: u32,
    pub complete: u32,
    pub partial: u32,
    pub rejected: u32,
}

pub struct ScanRequest<'a> {
    pub scope: &'a CarveScope,
    pub limits: CarveLimits,
    pub cancel: &'a AtomicBool,
    pub progress: &'a dyn Fn(u64, u64),
}
