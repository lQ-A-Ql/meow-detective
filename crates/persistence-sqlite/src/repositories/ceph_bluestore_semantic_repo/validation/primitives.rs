use crate::connection::{DbError, DbResult};

pub(super) fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn valid_hex_u64(value: &str) -> bool {
    value.len() == 16
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && u64::from_str_radix(value, 16).is_ok()
}

pub(super) fn parse_hex_u64(value: &str) -> Option<u64> {
    valid_hex_u64(value)
        .then(|| u64::from_str_radix(value, 16).ok())
        .flatten()
}

pub(super) fn valid_text(value: &str) -> bool {
    !value.is_empty() && !value.contains('\0')
}

pub(super) fn valid_optional_text(value: Option<&str>) -> bool {
    value.is_none_or(valid_text)
}

pub(super) fn valid_status(status: &str, reason: Option<&str>) -> bool {
    match status {
        "parsed" => reason.is_none(),
        "deferred" => reason.is_some_and(valid_deferred_reason),
        _ => false,
    }
}

fn valid_deferred_reason(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b':' | b'_' | b'-'))
}

pub(super) fn fits_sqlite(value: u64) -> bool {
    value <= i64::MAX as u64
}

pub(super) fn checked_len(length: usize) -> DbResult<u64> {
    u64::try_from(length)
        .ok()
        .filter(|count| fits_sqlite(*count))
        .ok_or_else(|| DbError::System("BlueStore semantic row count exceeds SQLite".to_string()))
}

pub(super) fn semantic_error<T>(message: &str) -> DbResult<T> {
    Err(DbError::System(message.to_string()))
}
