use crate::error::{LvmError, Result};

pub(super) fn checked_add(lhs: u64, rhs: u64, context: &str) -> Result<u64> {
    lhs.checked_add(rhs)
        .ok_or_else(|| LvmError::MetadataParseError {
            line: 0,
            message: format!("{context} overflows u64"),
        })
}

pub(super) fn checked_mul(lhs: u64, rhs: u64, context: &str) -> Result<u64> {
    lhs.checked_mul(rhs)
        .ok_or_else(|| LvmError::MetadataParseError {
            line: 0,
            message: format!("{context} overflows u64"),
        })
}
