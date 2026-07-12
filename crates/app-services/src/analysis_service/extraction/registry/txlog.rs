/// Determine whether any supplied transaction log parses successfully.
pub(super) fn merge_status(
    path: &str,
    txlog1: Option<&[u8]>,
    txlog2: Option<&[u8]>,
    warnings: &mut Vec<String>,
) -> bool {
    let mut merged = false;
    for (label, data) in [("LOG1", txlog1), ("LOG2", txlog2)] {
        if let Some(data) = data {
            match artifacts_windows::parse_transaction_log(data) {
                Ok(_) => merged = true,
                Err(err) => {
                    warnings.push(format!("{path} {label} txlog parse failed: {err}"));
                }
            }
        }
    }
    merged
}

/// Count deleted registry keys/values recovered from free cells.
pub(super) fn count_deleted_cells(path: &str, bytes: &[u8], warnings: &mut Vec<String>) -> u32 {
    match artifacts_windows::scan_deleted_registry_cells(bytes, path) {
        Ok(result) => (result.recovered_keys.len() + result.recovered_values.len()) as u32,
        Err(err) => {
            warnings.push(format!("{path} deleted cell scan failed: {err}"));
            0
        }
    }
}
