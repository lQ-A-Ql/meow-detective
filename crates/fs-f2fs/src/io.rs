use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use evidence_core::EvidenceReader;

use crate::{F2fsError, Result};

pub(crate) type SharedReader = Arc<Mutex<Box<dyn EvidenceReader>>>;

pub(crate) fn read_exact_at(source: &SharedReader, offset: u64, length: usize) -> Result<Vec<u8>> {
    let mut reader = source
        .lock()
        .map_err(|_| F2fsError::Invalid("evidence reader lock is poisoned".to_string()))?;
    reader.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0u8; length];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn block_offset(volume_offset: u64, block: u32) -> Result<u64> {
    u64::from(block)
        .checked_mul(crate::F2FS_BLOCK_SIZE as u64)
        .and_then(|offset| volume_offset.checked_add(offset))
        .ok_or_else(|| F2fsError::Invalid("F2FS block offset overflows".to_string()))
}

pub(crate) fn read_u16(bytes: &[u8], offset: usize, field: &str) -> Result<u16> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| F2fsError::Invalid(format!("truncated {field}")))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

pub(crate) fn read_u32(bytes: &[u8], offset: usize, field: &str) -> Result<u32> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| F2fsError::Invalid(format!("truncated {field}")))?;
    Ok(u32::from_le_bytes(value.try_into().map_err(|_| {
        F2fsError::Invalid(format!("truncated {field}"))
    })?))
}

pub(crate) fn read_u64(bytes: &[u8], offset: usize, field: &str) -> Result<u64> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| F2fsError::Invalid(format!("truncated {field}")))?;
    Ok(u64::from_le_bytes(value.try_into().map_err(|_| {
        F2fsError::Invalid(format!("truncated {field}"))
    })?))
}
