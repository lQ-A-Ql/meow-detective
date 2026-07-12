use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Cursor, Read, Seek, SeekFrom};

use super::{RegistryTransaction, RegistryTransactionOperation, TxLogParseResult};
use crate::registry::util::filetime_to_dt;
use crate::registry::RegistryError;

pub(crate) const HEADER_SIZE: u64 = 0x1000;
pub(crate) const MAGIC_HVLE: &[u8; 4] = b"HvLE";
pub(crate) const MAGIC_DIRT: &[u8; 4] = b"DIRT";
const MAX_ENTRY_BYTES: u32 = 1_048_576;
const MAX_KEY_PATH_CHARS: u16 = 32_767;
const MAX_VALUE_NAME_CHARS: u16 = 16_383;
const MIN_FILETIME: u64 = 125_911_584_000_000_000;
const MAX_FILETIME: u64 = 479_666_880_000_000_000;

pub fn parse_transaction_log(data: &[u8]) -> Result<TxLogParseResult, RegistryError> {
    if (data.len() as u64) < HEADER_SIZE {
        return Err(RegistryError::other(format!(
            "transaction log too short: {} bytes (minimum {HEADER_SIZE})",
            data.len()
        )));
    }

    let mut cursor = Cursor::new(data);
    let primary = read_header(&mut cursor)?;
    cursor
        .seek(SeekFrom::Start(HEADER_SIZE))
        .map_err(|error| RegistryError::other(format!("seek to entry region: {error}")))?;

    let mut transactions = Vec::new();
    let mut warnings = Vec::new();
    let mut state = SequenceState::default();
    let end = data.len() as u64;

    loop {
        match parse_next_entry(&mut cursor, end, &mut state, &mut warnings)? {
            EntryOutcome::Transaction(transaction) => transactions.push(transaction),
            EntryOutcome::Skipped => continue,
            EntryOutcome::End => break,
        }
    }

    Ok(TxLogParseResult {
        transactions,
        primary,
        warnings,
    })
}

#[derive(Default)]
struct SequenceState {
    last_sequence: Option<u64>,
    wraparound_warned: bool,
}

enum EntryOutcome {
    Transaction(RegistryTransaction),
    Skipped,
    End,
}

struct EntryData {
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
}

fn parse_next_entry(
    cursor: &mut Cursor<&[u8]>,
    end: u64,
    state: &mut SequenceState,
    warnings: &mut Vec<String>,
) -> Result<EntryOutcome, RegistryError> {
    let position = cursor
        .stream_position()
        .map_err(|error| RegistryError::other(format!("stream pos: {error}")))?;
    if position + 4 > end {
        return Ok(EntryOutcome::End);
    }
    let entry_size = match cursor.read_u32::<LittleEndian>() {
        Ok(size) if size != 0 => size,
        _ => return Ok(EntryOutcome::End),
    };
    let Some(record_end) = validate_entry_bounds(position, entry_size, end, warnings) else {
        return Ok(EntryOutcome::End);
    };
    let (sequence_number, operation_code, filetime) = read_fixed_entry_header(cursor)?;
    track_sequence(sequence_number, state, warnings);
    let Some(operation) = parse_operation(operation_code, position, record_end, cursor, warnings)?
    else {
        return Ok(EntryOutcome::Skipped);
    };
    let Some((key_path, value_name)) =
        read_entry_names(cursor, &operation, position, record_end, warnings)?
    else {
        return Ok(EntryOutcome::Skipped);
    };
    let data = read_entry_data(cursor, &operation, position)?;
    Ok(EntryOutcome::Transaction(RegistryTransaction {
        operation,
        key_path,
        value_name,
        data_before: data.before,
        data_after: data.after,
        sequence_number,
        timestamp: (MIN_FILETIME..=MAX_FILETIME)
            .contains(&filetime)
            .then(|| filetime_to_dt(filetime))
            .flatten(),
    }))
}

fn read_fixed_entry_header(cursor: &mut Cursor<&[u8]>) -> Result<(u64, u16, u64), RegistryError> {
    let sequence_number = cursor
        .read_u32::<LittleEndian>()
        .map_err(|error| RegistryError::other(format!("seq_num: {error}")))?
        as u64;
    let operation = cursor
        .read_u16::<LittleEndian>()
        .map_err(|error| RegistryError::other(format!("op: {error}")))?;
    let _flags = cursor
        .read_u16::<LittleEndian>()
        .map_err(|error| RegistryError::other(format!("flags: {error}")))?;
    let filetime = cursor
        .read_u64::<LittleEndian>()
        .map_err(|error| RegistryError::other(format!("timestamp: {error}")))?;
    Ok((sequence_number, operation, filetime))
}

fn track_sequence(sequence_number: u64, state: &mut SequenceState, warnings: &mut Vec<String>) {
    if let Some(previous) = state.last_sequence {
        if sequence_number < previous && !state.wraparound_warned {
            warnings.push(format!(
                "ring-buffer wraparound detected: sequence {previous} -> {sequence_number} (oldest entries were overwritten)"
            ));
            state.wraparound_warned = true;
        }
    }
    state.last_sequence = Some(sequence_number);
}

fn read_entry_names(
    cursor: &mut Cursor<&[u8]>,
    operation: &RegistryTransactionOperation,
    position: u64,
    record_end: u64,
    warnings: &mut Vec<String>,
) -> Result<Option<(String, Option<String>)>, RegistryError> {
    let Some(key_path) = read_bounded_string(
        cursor,
        MAX_KEY_PATH_CHARS,
        "key_path",
        "key-path",
        position,
        record_end,
        warnings,
    )?
    else {
        return Ok(None);
    };
    let Some(parsed_value_name) = read_bounded_string(
        cursor,
        MAX_VALUE_NAME_CHARS,
        "value_name",
        "value-name",
        position,
        record_end,
        warnings,
    )?
    else {
        return Ok(None);
    };
    let value_name = matches!(
        operation,
        RegistryTransactionOperation::SetValue | RegistryTransactionOperation::DeleteValue
    )
    .then_some(parsed_value_name)
    .filter(|name| !name.is_empty());
    Ok(Some((key_path, value_name)))
}

fn read_entry_data(
    cursor: &mut Cursor<&[u8]>,
    operation: &RegistryTransactionOperation,
    position: u64,
) -> Result<EntryData, RegistryError> {
    let before = read_data_blob(cursor).map_err(|error| {
        RegistryError::other(format!("data_before at offset {position:#x}: {error}"))
    })?;
    let after = read_data_blob(cursor).map_err(|error| {
        RegistryError::other(format!("data_after at offset {position:#x}: {error}"))
    })?;
    Ok(EntryData {
        before: matches!(
            operation,
            RegistryTransactionOperation::SetValue | RegistryTransactionOperation::DeleteValue
        )
        .then_some(before)
        .flatten(),
        after: matches!(
            operation,
            RegistryTransactionOperation::SetValue
                | RegistryTransactionOperation::CreateKey
                | RegistryTransactionOperation::RenameKey
        )
        .then_some(after)
        .flatten(),
    })
}

fn read_header(cursor: &mut Cursor<&[u8]>) -> Result<bool, RegistryError> {
    let mut magic = [0u8; 4];
    cursor
        .read_exact(&mut magic)
        .map_err(|error| RegistryError::other(format!("read magic: {error}")))?;
    let primary = if &magic == MAGIC_HVLE {
        true
    } else if &magic == MAGIC_DIRT {
        false
    } else {
        return Err(RegistryError::other(format!(
            "unrecognised transaction-log magic: {:02X?} (expected {:02X?} or {:02X?})",
            magic, MAGIC_HVLE, MAGIC_DIRT
        )));
    };
    let _sequence_one = cursor.read_u32::<LittleEndian>().unwrap_or(0);
    let _sequence_two = cursor.read_u32::<LittleEndian>().unwrap_or(0);
    let _flags = cursor.read_u32::<LittleEndian>().unwrap_or(0);
    Ok(primary)
}

fn validate_entry_bounds(
    position: u64,
    entry_size: u32,
    end: u64,
    warnings: &mut Vec<String>,
) -> Option<u64> {
    if entry_size < 24 {
        warnings.push(format!(
            "entry at offset {position:#x} has impossibly small size {entry_size}; stopping"
        ));
        return None;
    }
    if entry_size > MAX_ENTRY_BYTES {
        warnings.push(format!(
            "entry at offset {position:#x} size {entry_size} exceeds maximum {MAX_ENTRY_BYTES}; stopping"
        ));
        return None;
    }
    let record_end = position + entry_size as u64;
    if record_end > end {
        warnings.push(format!(
            "entry at offset {position:#x} with size {entry_size} extends past EOF; truncating"
        ));
        return None;
    }
    Some(record_end)
}

fn parse_operation(
    code: u16,
    position: u64,
    record_end: u64,
    cursor: &mut Cursor<&[u8]>,
    warnings: &mut Vec<String>,
) -> Result<Option<RegistryTransactionOperation>, RegistryError> {
    let operation = match code {
        0 => RegistryTransactionOperation::CreateKey,
        1 => RegistryTransactionOperation::DeleteKey,
        2 => RegistryTransactionOperation::SetValue,
        3 => RegistryTransactionOperation::DeleteValue,
        4 => RegistryTransactionOperation::RenameKey,
        other => {
            warnings.push(format!(
                "entry at offset {position:#x} has unknown operation type {other}; skipping"
            ));
            cursor
                .seek(SeekFrom::Start(record_end))
                .map_err(|error| RegistryError::other(format!("skip seek: {error}")))?;
            return Ok(None);
        }
    };
    Ok(Some(operation))
}

fn read_bounded_string(
    cursor: &mut Cursor<&[u8]>,
    maximum: u16,
    error_label: &str,
    warning_label: &str,
    position: u64,
    record_end: u64,
    warnings: &mut Vec<String>,
) -> Result<Option<String>, RegistryError> {
    let length = cursor
        .read_u16::<LittleEndian>()
        .map_err(|error| RegistryError::other(format!("{error_label}_len: {error}")))?;
    if length == 0 {
        return Ok(Some(String::new()));
    }
    if length > maximum {
        warnings.push(format!(
            "entry at offset {position:#x} {warning_label} len {length} exceeds limit"
        ));
        cursor
            .seek(SeekFrom::Start(record_end))
            .map_err(|error| RegistryError::other(format!("skip seek: {error}")))?;
        return Ok(None);
    }
    read_utf16_string(cursor, length as usize)
        .map(Some)
        .map_err(|error| {
            RegistryError::other(format!("{error_label} at offset {position:#x}: {error}"))
        })
}

fn read_utf16_string(
    cursor: &mut Cursor<&[u8]>,
    code_units: usize,
) -> Result<String, RegistryError> {
    let byte_len = code_units
        .checked_mul(2)
        .ok_or_else(|| RegistryError::utf16("UTF-16 byte length overflow"))?;
    let mut buffer = vec![0u8; byte_len];
    cursor
        .read_exact(&mut buffer)
        .map_err(|error| RegistryError::utf16(format!("read UTF-16 string: {error}")))?;
    let units = buffer
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    Ok(String::from_utf16_lossy(&units))
}

fn read_data_blob(cursor: &mut Cursor<&[u8]>) -> Result<Option<Vec<u8>>, RegistryError> {
    let length = cursor.read_u32::<LittleEndian>().map_err(|error| {
        RegistryError::truncated(
            cursor.position() as usize,
            format!("data blob len: {error}"),
        )
    })?;
    if length == 0 {
        return Ok(None);
    }
    if length > MAX_ENTRY_BYTES {
        return Err(RegistryError::truncated(
            cursor.position() as usize,
            format!("data blob length {length} exceeds maximum"),
        ));
    }
    let mut buffer = vec![0u8; length as usize];
    cursor.read_exact(&mut buffer).map_err(|error| {
        RegistryError::truncated(
            cursor.position() as usize,
            format!("read data blob: {error}"),
        )
    })?;
    Ok(Some(buffer))
}
