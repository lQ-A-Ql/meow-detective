use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use evidence_core::EvidenceReader;

use crate::{ErofsError, Result};

pub(crate) type SharedReader = Arc<Mutex<Box<dyn EvidenceReader>>>;

pub(crate) fn read_exact_at(source: &SharedReader, offset: u64, length: usize) -> Result<Vec<u8>> {
    let mut reader = source
        .lock()
        .map_err(|_| ErofsError::Invalid("evidence reader lock is poisoned".to_string()))?;
    reader.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0u8; length];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn block_offset(volume_offset: u64, block: u64, block_size: usize) -> Result<u64> {
    block
        .checked_mul(block_size as u64)
        .and_then(|offset| volume_offset.checked_add(offset))
        .ok_or_else(|| ErofsError::Invalid("EROFS block offset overflows".to_string()))
}

pub(crate) fn read_u16(bytes: &[u8], offset: usize, field: &str) -> Result<u16> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| ErofsError::Invalid(format!("truncated {field}")))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

pub(crate) fn read_u32(bytes: &[u8], offset: usize, field: &str) -> Result<u32> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| ErofsError::Invalid(format!("truncated {field}")))?;
    Ok(u32::from_le_bytes(value.try_into().map_err(|_| {
        ErofsError::Invalid(format!("truncated {field}"))
    })?))
}

pub(crate) fn read_u64(bytes: &[u8], offset: usize, field: &str) -> Result<u64> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| ErofsError::Invalid(format!("truncated {field}")))?;
    Ok(u64::from_le_bytes(value.try_into().map_err(|_| {
        ErofsError::Invalid(format!("truncated {field}"))
    })?))
}
