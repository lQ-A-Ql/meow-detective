use crate::error::{LvmError, Result};
use std::sync::{Mutex, MutexGuard};

pub(super) fn read_u32(data: &[u8], offset: usize, context: &str) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| metadata_error(format!("{context} offset overflows usize")))?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| metadata_error(format!("{context} is outside buffer")))?;
    let bytes = bytes
        .try_into()
        .map_err(|_| metadata_error(format!("{context} must be exactly 4 bytes")))?;
    Ok(u32::from_le_bytes(bytes))
}

pub(super) fn read_u64(data: &[u8], offset: usize, context: &str) -> Result<u64> {
    let end = offset
        .checked_add(8)
        .ok_or_else(|| metadata_error(format!("{context} offset overflows usize")))?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| metadata_error(format!("{context} is outside buffer")))?;
    let bytes = bytes
        .try_into()
        .map_err(|_| metadata_error(format!("{context} must be exactly 8 bytes")))?;
    Ok(u64::from_le_bytes(bytes))
}

pub(super) fn checked_add(lhs: u64, rhs: u64, context: &str) -> Result<u64> {
    lhs.checked_add(rhs)
        .ok_or_else(|| metadata_error(format!("{context} overflows u64")))
}

pub(super) fn checked_mul(lhs: u64, rhs: u64, context: &str) -> Result<u64> {
    lhs.checked_mul(rhs)
        .ok_or_else(|| metadata_error(format!("{context} overflows u64")))
}

pub(super) fn metadata_error(message: String) -> LvmError {
    LvmError::MetadataParseError { line: 0, message }
}

pub(super) fn lock_mutex<'a, T>(mutex: &'a Mutex<T>, context: &str) -> Result<MutexGuard<'a, T>> {
    mutex
        .lock()
        .map_err(|_| metadata_error(format!("{context} lock poisoned")))
}
