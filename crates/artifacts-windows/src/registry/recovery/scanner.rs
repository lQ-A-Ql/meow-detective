use super::bytes::{read_i32, read_u32};
use super::constants::{BASE_BLOCK_SIZE, HBIN_HEADER_SIZE, HBIN_MAGIC, NK_SIGNATURE, VK_SIGNATURE};
use super::records::{try_recover_nk, try_recover_vk};
use super::{FreeCell, RecoverResult, RecoveredKey, RecoveredValue};
use crate::registry::RegistryError;

pub fn scan_free_cells(bytes: &[u8]) -> Vec<FreeCell> {
    let mut cells = Vec::new();
    let mut hbin_offset = BASE_BLOCK_SIZE;

    while hbin_offset + HBIN_HEADER_SIZE <= bytes.len() {
        if bytes.get(hbin_offset..hbin_offset + 4) != Some(HBIN_MAGIC) {
            break;
        }
        let hbin_size = match read_u32(bytes, hbin_offset + 8) {
            Ok(size) => size as usize,
            Err(_) => break,
        };
        if hbin_size == 0 || hbin_size % BASE_BLOCK_SIZE != 0 {
            break;
        }
        let hbin_end = hbin_offset.saturating_add(hbin_size).min(bytes.len());
        let mut cell_position = hbin_offset + HBIN_HEADER_SIZE;

        while cell_position + 4 <= hbin_end {
            let cell_size = match read_i32(bytes, cell_position) {
                Ok(size) => size,
                Err(_) => break,
            };
            if cell_size == 0 {
                break;
            }
            if cell_size > 0 {
                cells.push(FreeCell {
                    size: cell_size as usize,
                    offset: cell_position,
                });
            }
            let step = cell_size.unsigned_abs() as usize;
            if step == 0 {
                break;
            }
            cell_position = cell_position.saturating_add(step);
        }
        hbin_offset = hbin_offset.saturating_add(hbin_size);
    }
    cells
}

pub fn scan_deleted_registry_cells(
    bytes: &[u8],
    hive_path: &str,
) -> Result<RecoverResult, RegistryError> {
    if bytes.len() < BASE_BLOCK_SIZE {
        return Err(RegistryError::invalid_cell(
            "registry hive too short for base block",
        ));
    }
    if bytes.get(0..4) != Some(b"regf") {
        return Err(RegistryError::invalid_cell(
            "not a valid registry hive (missing 'regf' magic)",
        ));
    }

    let free_cells = scan_free_cells(bytes);
    let mut recovered_keys: Vec<RecoveredKey> = Vec::new();
    let mut recovered_values: Vec<RecoveredValue> = Vec::new();
    for cell in &free_cells {
        let data_start = cell.offset + 4;
        let data_len = cell.size.saturating_sub(4);
        if data_len < 4 {
            continue;
        }
        match bytes.get(data_start..data_start + 2) {
            Some(signature) if signature == NK_SIGNATURE.as_slice() => {
                if let Some(key) = try_recover_nk(bytes, cell, data_start, data_len, hive_path) {
                    recovered_keys.push(key);
                }
            }
            Some(signature) if signature == VK_SIGNATURE.as_slice() => {
                if let Some(value) = try_recover_vk(bytes, cell, data_start, data_len, hive_path) {
                    recovered_values.push(value);
                }
            }
            _ => {}
        }
    }

    Ok(RecoverResult {
        recovered_keys,
        recovered_values,
        free_cells_scanned: free_cells.len() as u32,
    })
}
