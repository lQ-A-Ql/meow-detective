use super::types::{ParsedRegistryField, TxlogTimestampInfo, USER_ASSIST_ENTRY_SIZE};
use crate::registry::txlog::{RegistryTransaction, RegistryTransactionOperation};

/// Attempt to override a [`ParsedRegistryField`] with a more recent value from
/// the transaction log.  Returns a [`TxlogTimestampInfo`] describing whether an
/// override was applied.
pub(crate) fn apply_single_txlog_override(
    field: &mut ParsedRegistryField,
    transactions: &[RegistryTransaction],
) -> TxlogTimestampInfo {
    let best = find_best_txlog_match(transactions, &field.key_path, &field.value_name);

    match best {
        Some(txn) => {
            let old_value = field.value.clone();
            field.value = txn
                .data_after
                .as_deref()
                .and_then(txlog_data_to_string)
                .unwrap_or(old_value);
            TxlogTimestampInfo {
                field_name: field.value_name.clone(),
                hive_timestamp: None,
                txlog_timestamp: txn.timestamp,
                txlog_used: true,
            }
        }
        None => TxlogTimestampInfo {
            field_name: field.value_name.clone(),
            hive_timestamp: None,
            txlog_timestamp: None,
            txlog_used: false,
        },
    }
}

/// Search transaction-log entries for the best `SetValue` matching `key_path`
/// and `value_name`.  "Best" means the highest sequence number among matches.
pub(crate) fn find_best_txlog_match<'a>(
    transactions: &'a [RegistryTransaction],
    key_path: &str,
    value_name: &str,
) -> Option<&'a RegistryTransaction> {
    transactions
        .iter()
        .filter(|txn| {
            txn.operation == RegistryTransactionOperation::SetValue
                && txn.value_name.as_deref() == Some(value_name)
                && txlog_key_path_matches(&txn.key_path, key_path)
        })
        .max_by_key(|txn| txn.sequence_number)
}

/// Check whether a transaction-log key path matches a hive-relative key path.
///
/// Registry hives are mounted at well-known roots (`\Registry\Machine\SYSTEM`,
/// `\Registry\Machine\SOFTWARE`, `\Registry\User\...`).  The txlog records the
/// full absolute path, while the hive extractor stores the path relative to the
/// hive root.  This function strips common prefixes and compares suffixes
/// case-insensitively.
pub(crate) fn txlog_key_path_matches(txlog_path: &str, hive_key_path: &str) -> bool {
    let tx = txlog_path.trim_matches('\\').to_lowercase();
    let hi = hive_key_path.trim_matches('\\').to_lowercase();

    if tx == hi {
        return true;
    }

    // Suffix match: the hive-relative path should be a suffix of the absolute
    // txlog path, separated by a backslash.
    if tx.len() > hi.len() {
        let split_at = tx.len() - hi.len();
        if split_at > 0
            && tx.as_bytes().get(split_at - 1) == Some(&b'\\')
            && tx.as_bytes()[split_at..] == hi.as_bytes()[..]
        {
            return true;
        }
    }

    false
}

/// Search txlog entries for a match on a UserAssist Count subkey.
///
/// UserAssist txlog entries have key paths like
/// `\Registry\User\...\UserAssist\{GUID}\Count` and value names that are the
/// ROT13-encoded executable path.
pub(crate) fn find_best_txlog_match_user_assist<'a>(
    transactions: &'a [RegistryTransaction],
    encoded_value_name: &str,
) -> Option<&'a RegistryTransaction> {
    transactions
        .iter()
        .filter(|txn| {
            txn.operation == RegistryTransactionOperation::SetValue
                && txn.value_name.as_deref() == Some(encoded_value_name)
                && txn.key_path.to_lowercase().contains("\\userassist\\")
                && txn.key_path.to_lowercase().ends_with("\\count")
        })
        .max_by_key(|txn| txn.sequence_number)
}

/// Parse a 72-byte UserAssist binary blob into its constituent fields.
///
/// Returns `(run_count, session_id, focus_time_ms, filetime)` on success, or
/// `None` if the data is too short.
pub(crate) fn parse_user_assist_binary(data: &[u8]) -> Option<(u32, u32, u32, u64)> {
    if data.len() < USER_ASSIST_ENTRY_SIZE {
        return None;
    }
    let run_count = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let session_id = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let focus_time_ms = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let filetime = u64::from_le_bytes([
        data[60], data[61], data[62], data[63], data[64], data[65], data[66], data[67],
    ]);
    Some((run_count, session_id, focus_time_ms, filetime))
}

/// Convert raw registry-value bytes (as recorded in the transaction log) to a
/// string.  Registry string types are stored as UTF-16LE; this attempts that
/// decode first and falls back to UTF-8 / Latin-1.
pub(crate) fn txlog_data_to_string(data: &[u8]) -> Option<String> {
    if data.is_empty() {
        return None;
    }
    // Primary path: UTF-16LE (registry REG_SZ / REG_EXPAND_SZ wire format).
    if data.len() >= 2 && data.len().is_multiple_of(2) {
        let units: Vec<u16> = data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let s = String::from_utf16_lossy(&units);
        let trimmed = s.trim_end_matches('\0').to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    // Fallback: raw UTF-8.
    String::from_utf8(data.to_vec()).ok()
}
