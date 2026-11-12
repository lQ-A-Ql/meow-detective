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
