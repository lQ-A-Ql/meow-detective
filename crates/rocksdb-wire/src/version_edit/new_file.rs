use std::collections::BTreeSet;

use crate::cursor::WireCursor;
use crate::error::{Result, RocksDbWireError};
use crate::limits::VersionEditLimits;

use super::model::{InternalKeyMetadata, NewFile, NewFileFormat, NewFileMetadata};
use super::tags::{
    CUSTOM_COMPENSATED_RANGE_DELETION_SIZE, CUSTOM_EPOCH_NUMBER, CUSTOM_FILE_CHECKSUM,
    CUSTOM_FILE_CHECKSUM_FUNCTION, CUSTOM_FILE_CREATION_TIME, CUSTOM_MANDATORY_MASK,
    CUSTOM_MAX_TIMESTAMP, CUSTOM_MIN_LOG_NUMBER_HACK, CUSTOM_MIN_TIMESTAMP, CUSTOM_NEED_COMPACTION,
    CUSTOM_OLDEST_ANCESTOR_TIME, CUSTOM_OLDEST_BLOB_FILE, CUSTOM_PATH_ID, CUSTOM_TEMPERATURE,
    CUSTOM_TERMINATE, CUSTOM_UNIQUE_ID, FILE_NUMBER_MASK, MAX_SEQUENCE_NUMBER,
};

const REEF_PSEUDO_KEY: &[u8] = b"dummy_key\x01\x00\x00\x00\x00\x00\x00\x00";
const MAX_TEMPERATURE_VALUE: u8 = 0x0c;

#[derive(Debug)]
struct ParsedInternalKey<'a> {
    metadata: InternalKeyMetadata,
    encoded: &'a [u8],
}

pub(super) fn parse_new_file(
    format: NewFileFormat,
    cursor: &mut WireCursor<'_>,
    limits: VersionEditLimits,
    min_log_number_to_keep: &mut Option<u64>,
) -> Result<Option<NewFile>> {
    let level = parse_level(cursor, limits)?;
    let file_number = cursor.read_varint_u64("new file number")?;
    let mut path_id = 0u32;
    if format == NewFileFormat::NewFile3 {
        path_id = cursor.read_varint_u32("new file path ID")?;
        validate_path_id(path_id)?;
    }
    let file_size = cursor.read_varint_u64("new file size")?;
    let smallest = parse_internal_key(cursor, "smallest internal key", limits)?;
    let largest = parse_internal_key(cursor, "largest internal key", limits)?;
    let (smallest_sequence_number, largest_sequence_number) =
        parse_file_sequence_range(format, cursor)?;

    let mut metadata = NewFileMetadata::default();
    if format == NewFileFormat::NewFile4 {
        path_id = parse_custom_fields(cursor, limits, min_log_number_to_keep, &mut metadata)?;
    }

    if is_reef_pseudo_record(
        format,
        level,
        file_number,
        file_size,
        smallest.encoded,
        largest.encoded,
        min_log_number_to_keep.is_some(),
    ) {
        return Ok(None);
    }
    validate_file_number(file_number)?;
    validate_file_sequence_range(format, smallest_sequence_number, largest_sequence_number)?;
    Ok(Some(NewFile {
        format,
        level,
        file_number,
        path_id,
        file_size,
        smallest: smallest.metadata,
        largest: largest.metadata,
        smallest_sequence_number,
        largest_sequence_number,
        metadata,
    }))
}

pub(super) fn parse_level(cursor: &mut WireCursor<'_>, limits: VersionEditLimits) -> Result<u32> {
    let level = cursor.read_varint_u32("level")?;
    if level > limits.max_level {
        return Err(RocksDbWireError::InvalidLevel {
            level,
            max_level: limits.max_level,
        });
    }
    Ok(level)
}

pub(super) fn parse_internal_key_metadata(
    cursor: &mut WireCursor<'_>,
    context: &'static str,
    limits: VersionEditLimits,
) -> Result<InternalKeyMetadata> {
    Ok(parse_internal_key(cursor, context, limits)?.metadata)
}

fn parse_internal_key<'a>(
    cursor: &mut WireCursor<'a>,
    context: &'static str,
    limits: VersionEditLimits,
) -> Result<ParsedInternalKey<'a>> {
    let encoded = cursor.read_length_prefixed(context, limits.max_internal_key_bytes)?;
    if encoded.len() < 8 {
        return Err(RocksDbWireError::InternalKeyTooShort {
            context,
            length: encoded.len(),
        });
    }
    let footer_start = encoded.len() - 8;
    let footer = u64::from_le_bytes(encoded[footer_start..].try_into().map_err(|_| {
        RocksDbWireError::InternalKeyTooShort {
            context,
            length: encoded.len(),
        }
    })?);
    let value_type = footer as u8;
    if !is_extended_value_type(value_type) {
        return Err(RocksDbWireError::InvalidInternalKeyType {
            context,
            value_type,
        });
    }
    Ok(ParsedInternalKey {
        metadata: InternalKeyMetadata {
            encoded_length: encoded.len() as u32,
            user_key_length: footer_start as u32,
            sequence_number: footer >> 8,
            value_type,
        },
        encoded,
    })
}

fn parse_file_sequence_range(
    format: NewFileFormat,
    cursor: &mut WireCursor<'_>,
) -> Result<(u64, u64)> {
    if format == NewFileFormat::NewFile {
        return Ok((MAX_SEQUENCE_NUMBER, 0));
    }
    Ok((
        cursor.read_varint_u64("smallest file sequence number")?,
        cursor.read_varint_u64("largest file sequence number")?,
    ))
}

fn parse_custom_fields(
    cursor: &mut WireCursor<'_>,
    limits: VersionEditLimits,
    min_log_number_to_keep: &mut Option<u64>,
    metadata: &mut NewFileMetadata,
) -> Result<u32> {
    let mut seen = BTreeSet::new();
    let mut path_id = 0u32;
    let mut count = 0usize;
    loop {
        let tag = cursor.read_varint_u32("NewFile4 custom tag")?;
        if tag == CUSTOM_TERMINATE {
            return Ok(path_id);
        }
        if count >= limits.max_custom_fields_per_file {
            return Err(RocksDbWireError::CustomFieldCountLimit {
                limit: limits.max_custom_fields_per_file,
            });
        }
        count += 1;
        let field =
            cursor.read_length_prefixed("NewFile4 custom field", limits.max_custom_field_bytes)?;
        if is_known_custom_tag(tag) && !seen.insert(tag) {
            return Err(RocksDbWireError::DuplicateCustomField { tag });
        }
        decode_custom_field(
            tag,
            field,
            cursor.position() - field.len(),
            min_log_number_to_keep,
            metadata,
            &mut path_id,
        )?;
    }
}

fn decode_custom_field(
    tag: u32,
    field: &[u8],
    field_offset: usize,
    min_log_number_to_keep: &mut Option<u64>,
    metadata: &mut NewFileMetadata,
    path_id: &mut u32,
) -> Result<()> {
    match tag {
        CUSTOM_NEED_COMPACTION => {
            metadata.marked_for_compaction = read_single_byte(field, "need compaction")? == 1;
        }
        CUSTOM_MIN_LOG_NUMBER_HACK => {
            let value = read_fixed_u64_field(field, field_offset, "minimum log number hack")?;
            set_consistent_min_log(min_log_number_to_keep, value)?;
        }
        CUSTOM_OLDEST_BLOB_FILE => {
            metadata.oldest_blob_file_number = Some(read_varint_field(
                field,
                field_offset,
                "oldest blob file number",
            )?);
        }
        CUSTOM_OLDEST_ANCESTOR_TIME => {
            metadata.oldest_ancestor_time = Some(read_varint_field(
                field,
                field_offset,
                "oldest ancestor time",
            )?);
        }
        CUSTOM_FILE_CREATION_TIME => {
            metadata.file_creation_time = Some(read_varint_field(
                field,
                field_offset,
                "file creation time",
            )?);
        }
        CUSTOM_FILE_CHECKSUM => metadata.file_checksum_length = Some(field.len() as u32),
        CUSTOM_FILE_CHECKSUM_FUNCTION => {
            metadata.file_checksum_function_length = Some(field.len() as u32);
        }
        CUSTOM_TEMPERATURE => {
            let value = read_single_byte(field, "temperature")?;
            if value <= MAX_TEMPERATURE_VALUE {
                metadata.temperature = Some(value);
            }
        }
        CUSTOM_MIN_TIMESTAMP => metadata.min_timestamp_length = Some(field.len() as u32),
        CUSTOM_MAX_TIMESTAMP => metadata.max_timestamp_length = Some(field.len() as u32),
        CUSTOM_UNIQUE_ID => metadata.unique_id_length = Some(field.len() as u32),
        CUSTOM_EPOCH_NUMBER => {
            metadata.epoch_number = Some(read_varint_field(field, field_offset, "epoch number")?);
        }
        CUSTOM_COMPENSATED_RANGE_DELETION_SIZE => {
            metadata.compensated_range_deletion_size = Some(read_varint_field(
                field,
                field_offset,
                "compensated range deletion size",
            )?);
        }
        CUSTOM_PATH_ID => {
            let value = u32::from(read_single_byte(field, "path ID")?);
            validate_path_id(value)?;
            *path_id = value;
        }
        _ if tag & CUSTOM_MANDATORY_MASK != 0 => {
            return Err(RocksDbWireError::UnknownMandatoryCustomTag { tag });
        }
        _ => metadata.skipped_safe_custom_fields += 1,
    }
    Ok(())
}

fn read_varint_field(field: &[u8], field_offset: usize, context: &'static str) -> Result<u64> {
    let mut cursor = WireCursor::new_at(field, field_offset);
    let value = cursor.read_varint_u64(context)?;
    if !cursor.is_empty() {
        return Err(RocksDbWireError::InvalidField {
            context,
            reason: "trailing bytes",
        });
    }
    Ok(value)
}

fn read_fixed_u64_field(field: &[u8], field_offset: usize, context: &'static str) -> Result<u64> {
    let mut cursor = WireCursor::new_at(field, field_offset);
    let value = cursor.read_fixed_u64(context)?;
    if !cursor.is_empty() {
        return Err(RocksDbWireError::InvalidField {
            context,
            reason: "fixed64 width",
        });
    }
    Ok(value)
}

fn read_single_byte(field: &[u8], context: &'static str) -> Result<u8> {
    if field.len() != 1 {
        return Err(RocksDbWireError::InvalidField {
            context,
            reason: "expected one byte",
        });
    }
    Ok(field[0])
}

fn validate_file_number(file_number: u64) -> Result<()> {
    if file_number == 0 || file_number > FILE_NUMBER_MASK {
        return Err(RocksDbWireError::InvalidFileNumber { file_number });
    }
    Ok(())
}

fn validate_path_id(path_id: u32) -> Result<()> {
    if path_id > 3 {
        return Err(RocksDbWireError::InvalidPathId { path_id });
    }
    Ok(())
}

fn validate_file_sequence_range(format: NewFileFormat, smallest: u64, largest: u64) -> Result<()> {
    if format == NewFileFormat::NewFile {
        return Ok(());
    }
    for sequence in [smallest, largest] {
        if sequence > MAX_SEQUENCE_NUMBER {
            return Err(RocksDbWireError::InvalidSequenceNumber { sequence });
        }
    }
    if smallest > largest {
        return Err(RocksDbWireError::InvalidSequenceRange { smallest, largest });
    }
    Ok(())
}

fn set_consistent_min_log(target: &mut Option<u64>, value: u64) -> Result<()> {
    if let Some(existing) = *target {
        if existing != value {
            return Err(RocksDbWireError::ConflictingField {
                field: "minimum log number to keep",
                first: existing,
                second: value,
            });
        }
    } else {
        *target = Some(value);
    }
    Ok(())
}

fn is_extended_value_type(value_type: u8) -> bool {
    matches!(
        value_type,
        0x00 | 0x01 | 0x02 | 0x07 | 0x0f | 0x11 | 0x14 | 0x16
    )
}

fn is_known_custom_tag(tag: u32) -> bool {
    matches!(
        tag,
        CUSTOM_NEED_COMPACTION
            | CUSTOM_MIN_LOG_NUMBER_HACK
            | CUSTOM_OLDEST_BLOB_FILE
            | CUSTOM_OLDEST_ANCESTOR_TIME
            | CUSTOM_FILE_CREATION_TIME
            | CUSTOM_FILE_CHECKSUM
            | CUSTOM_FILE_CHECKSUM_FUNCTION
            | CUSTOM_TEMPERATURE
            | CUSTOM_MIN_TIMESTAMP
            | CUSTOM_MAX_TIMESTAMP
            | CUSTOM_UNIQUE_ID
            | CUSTOM_EPOCH_NUMBER
            | CUSTOM_COMPENSATED_RANGE_DELETION_SIZE
            | CUSTOM_PATH_ID
    )
}

fn is_reef_pseudo_record(
    format: NewFileFormat,
    level: u32,
    file_number: u64,
    file_size: u64,
    smallest: &[u8],
    largest: &[u8],
    has_min_log_number_to_keep: bool,
) -> bool {
    format == NewFileFormat::NewFile4
        && level == 0
        && file_number == 0
        && file_size == 0
        && has_min_log_number_to_keep
        && smallest == REEF_PSEUDO_KEY
        && largest == REEF_PSEUDO_KEY
}
