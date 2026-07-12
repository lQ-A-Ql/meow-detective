use super::bytes::{decode_utf16le_lossy, format_hex_preview, read_i32};
use super::constants::{
    BASE_BLOCK_SIZE, INVALID_OFFSET, MAX_FILETIME, MAX_NAME_BYTES, MIN_FILETIME, MIN_NK_BODY,
    MIN_VK_BODY, NK_SIGNATURE,
};
use super::{FreeCell, RecoveredKey, RecoveredValue};
use crate::registry::util::filetime_to_dt;

pub(super) fn try_recover_nk(
    bytes: &[u8],
    cell: &FreeCell,
    data_start: usize,
    data_len: usize,
    hive_path: &str,
) -> Option<RecoveredKey> {
    let _ = hive_path;
    if data_len < MIN_NK_BODY {
        return None;
    }
    let cell_data = &bytes[data_start..data_start + data_len.min(512)];
    let flags = read_u16_at(cell_data, 2).unwrap_or(0);
    let last_written_raw = read_u64_at(cell_data, 4).unwrap_or(0);
    let last_written = (MIN_FILETIME..=MAX_FILETIME)
        .contains(&last_written_raw)
        .then(|| filetime_to_dt(last_written_raw))
        .flatten();
    let parent_offset = read_u32_at(cell_data, 12).unwrap_or(INVALID_OFFSET);
    let num_values = read_u32_at(cell_data, 0x24).unwrap_or(0);
    let name_len = read_u16_at(cell_data, 0x48).unwrap_or(0) as usize;
    let bounded_name_len = name_len.min(MAX_NAME_BYTES);
    let name_start: usize = 0x4c;
    let name_bytes = cell_data
        .get(name_start..name_start.saturating_add(bounded_name_len))
        .unwrap_or_default();
    let key_name = decode_name(name_bytes, flags & 0x20 != 0);
    let low_confidence = bounded_name_len > 0 && name_start + bounded_name_len > data_len
        || key_name.is_empty()
        || key_name.chars().all(|character| character == '\u{fffd}')
        || last_written.is_none() && last_written_raw != 0;

    Some(RecoveredKey {
        key_name,
        last_written,
        num_values,
        cell_offset: cell.offset.saturating_sub(BASE_BLOCK_SIZE) as u32,
        parent_path_hint: resolve_parent_name(bytes, parent_offset),
        confidence: if low_confidence { "low" } else { "high" },
    })
}

pub(super) fn try_recover_vk(
    bytes: &[u8],
    _cell: &FreeCell,
    data_start: usize,
    data_len: usize,
    hive_path: &str,
) -> Option<RecoveredValue> {
    let _ = hive_path;
    if data_len < MIN_VK_BODY {
        return None;
    }
    let cell_data = &bytes[data_start..data_start + data_len.min(512)];
    let name_len = read_u16_at(cell_data, 2).unwrap_or(0) as usize;
    let bounded_name_len = name_len.min(MAX_NAME_BYTES);
    let data_len_raw = read_u32_at(cell_data, 4).unwrap_or(0);
    let data_offset = read_u32_at(cell_data, 8).unwrap_or(INVALID_OFFSET);
    let value_type = read_u32_at(cell_data, 12).unwrap_or(0);
    let flags = read_u16_at(cell_data, 16).unwrap_or(0);
    let name_start: usize = 0x14;
    let name_bytes = cell_data
        .get(name_start..name_start.saturating_add(bounded_name_len))
        .unwrap_or_default();
    let value_name = decode_name(name_bytes, flags & 0x01 != 0);
    let low_confidence = bounded_name_len > 0 && name_start + bounded_name_len > data_len
        || value_name.is_empty()
        || value_name.chars().all(|character| character == '\u{fffd}')
        || value_type > 100;

    Some(RecoveredValue {
        value_name,
        value_type,
        value_data_preview: read_value_data_preview(bytes, data_len_raw, data_offset),
        key_path_hint: String::new(),
        confidence: if low_confidence { "low" } else { "high" },
    })
}

fn read_value_data_preview(bytes: &[u8], data_len_raw: u32, data_offset: u32) -> String {
    let actual_len = data_len_raw & 0x7fff_ffff;
    if actual_len == 0 {
        return "(empty)".to_string();
    }
    let preview_len = (actual_len as usize).min(128);
    let inline_buffer = data_offset.to_le_bytes();
    let preview = if data_len_raw & 0x8000_0000 != 0 {
        &inline_buffer[..preview_len.min(4)]
    } else {
        let data_absolute = BASE_BLOCK_SIZE.saturating_add(data_offset as usize);
        if data_absolute + 8 > bytes.len() {
            return "(data cell out of bounds)".to_string();
        }
        let cell_size = match read_i32(bytes, data_absolute) {
            Ok(size) => size,
            Err(_) => return "(data cell unreadable)".to_string(),
        };
        let data_start = data_absolute + 4;
        if data_start + preview_len > bytes.len() {
            return "(data cell truncated)".to_string();
        }
        let available = preview_len.min((cell_size.unsigned_abs() as usize).saturating_sub(4));
        if available == 0 {
            return "(data cell too small)".to_string();
        }
        &bytes[data_start..data_start + available]
    };
    format_hex_preview(preview)
}

fn resolve_parent_name(bytes: &[u8], parent_offset: u32) -> String {
    if parent_offset == INVALID_OFFSET || parent_offset == 0 {
        return "(orphan)".to_string();
    }
    let absolute = BASE_BLOCK_SIZE.saturating_add(parent_offset as usize);
    if absolute + 8 > bytes.len() {
        return format!("(parent at 0x{parent_offset:X}, out of bounds)");
    }
    let cell_size = match read_i32(bytes, absolute) {
        Ok(size) => size,
        Err(_) => return format!("(parent at 0x{parent_offset:X}, unreadable)"),
    };
    if cell_size >= 0 {
        if cell_size.unsigned_abs() as usize >= 0x54
            && bytes.get(absolute + 4..absolute + 6) == Some(NK_SIGNATURE)
        {
            let name = try_read_nk_name_fast(bytes, absolute);
            if !name.is_empty() {
                return format!("(deleted) {name}");
            }
        }
        return format!("(deleted, parent at 0x{parent_offset:X})");
    }
    if bytes.get(absolute + 4..absolute + 6) != Some(NK_SIGNATURE) {
        return format!("(parent at 0x{parent_offset:X}, not an NK cell)");
    }
    let name = try_read_nk_name_fast(bytes, absolute);
    if name.is_empty() {
        format!("(parent at 0x{parent_offset:X})")
    } else {
        name
    }
}

fn try_read_nk_name_fast(bytes: &[u8], cell_absolute: usize) -> String {
    if cell_absolute + 0x50 > bytes.len() {
        return String::new();
    }
    let flags = read_u16_at(bytes, cell_absolute + 6).unwrap_or(0);
    let name_len = read_u16_at(bytes, cell_absolute + 0x4c).unwrap_or(0) as usize;
    let name_len = name_len.min(MAX_NAME_BYTES);
    if name_len == 0 {
        return String::new();
    }
    let Some(name_bytes) = bytes.get(cell_absolute + 0x50..cell_absolute + 0x50 + name_len) else {
        return String::new();
    };
    decode_name(name_bytes, flags & 0x20 != 0)
}

fn decode_name(bytes: &[u8], compressed: bool) -> String {
    if bytes.is_empty() {
        String::new()
    } else if compressed {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        decode_utf16le_lossy(bytes)
    }
}

fn read_u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_u64_at(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}
