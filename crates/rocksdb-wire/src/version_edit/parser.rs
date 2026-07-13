use crate::cursor::WireCursor;
use crate::error::{Result, RocksDbWireError};
use crate::limits::VersionEditLimits;

use super::model::{
    ColumnFamilyAction, CompactCursor, DeletedFile, IgnoredField, NewFileFormat, VersionEdit,
};
use super::new_file::{parse_internal_key_metadata, parse_level, parse_new_file};
use super::tags::{
    COLUMN_FAMILY, COLUMN_FAMILY_ADD, COLUMN_FAMILY_DROP, COMPACT_CURSOR, COMPARATOR, DELETED_FILE,
    FILE_NUMBER_MASK, IN_ATOMIC_GROUP, LAST_SEQUENCE, LOG_NUMBER, MAX_COLUMN_FAMILY,
    MAX_SEQUENCE_NUMBER, MIN_LOG_NUMBER_TO_KEEP, NEW_FILE, NEW_FILE2, NEW_FILE3, NEW_FILE4,
    NEXT_FILE_NUMBER, PREVIOUS_LOG_NUMBER, SAFE_IGNORE_MASK,
};

#[derive(Debug, Default)]
struct SingularFields {
    comparator: bool,
    log_number: bool,
    previous_log_number: bool,
    next_file_number: bool,
    last_sequence: bool,
    min_log_number_to_keep: bool,
    max_column_family: bool,
    column_family: bool,
    column_family_action: bool,
    atomic_group: bool,
}

pub fn parse_version_edit(input: &[u8], limits: VersionEditLimits) -> Result<VersionEdit> {
    if input.len() > limits.max_edit_bytes {
        return Err(RocksDbWireError::VersionEditLengthLimit {
            length: input.len(),
            limit: limits.max_edit_bytes,
        });
    }
    let mut cursor = WireCursor::new(input);
    let mut edit = VersionEdit::default();
    let mut seen = SingularFields::default();
    let mut tag_count = 0usize;
    let mut mutation_count = 0usize;

    while !cursor.is_empty() {
        tag_count += 1;
        if tag_count > limits.max_tags {
            return Err(RocksDbWireError::VersionEditTagLimit {
                limit: limits.max_tags,
            });
        }
        let tag = cursor.read_varint_u32("VersionEdit tag")?;
        decode_tag(
            tag,
            &mut cursor,
            limits,
            &mut edit,
            &mut seen,
            &mut mutation_count,
        )?;
    }
    validate_edit_shape(&edit, mutation_count)?;
    edit.decoded_tag_count = tag_count as u32;
    Ok(edit)
}

fn decode_tag(
    tag: u32,
    cursor: &mut WireCursor<'_>,
    limits: VersionEditLimits,
    edit: &mut VersionEdit,
    seen: &mut SingularFields,
    mutation_count: &mut usize,
) -> Result<()> {
    match tag {
        COMPARATOR => {
            ensure_once(&mut seen.comparator, "comparator")?;
            edit.comparator = Some(read_bytes(cursor, "comparator", limits)?);
        }
        LOG_NUMBER => {
            ensure_once(&mut seen.log_number, "log number")?;
            edit.log_number = Some(read_file_meta_number(cursor, "log number", true)?);
        }
        PREVIOUS_LOG_NUMBER => {
            ensure_once(&mut seen.previous_log_number, "previous log number")?;
            edit.previous_log_number =
                Some(read_file_meta_number(cursor, "previous log number", true)?);
        }
        NEXT_FILE_NUMBER => {
            ensure_once(&mut seen.next_file_number, "next file number")?;
            edit.next_file_number = Some(read_file_meta_number(cursor, "next file number", false)?);
        }
        LAST_SEQUENCE => {
            ensure_once(&mut seen.last_sequence, "last sequence")?;
            edit.last_sequence = Some(read_sequence(cursor, "last sequence")?);
        }
        MIN_LOG_NUMBER_TO_KEEP => {
            ensure_once(
                &mut seen.min_log_number_to_keep,
                "minimum log number to keep",
            )?;
            let value = read_file_meta_number(cursor, "minimum log number to keep", true)?;
            set_consistent_number(
                &mut edit.min_log_number_to_keep,
                value,
                "minimum log number to keep",
            )?;
        }
        MAX_COLUMN_FAMILY => {
            ensure_once(&mut seen.max_column_family, "maximum column family")?;
            edit.max_column_family_id = Some(cursor.read_varint_u32("maximum column family")?);
        }
        COMPACT_CURSOR => parse_compact_cursor(cursor, limits, edit)?,
        DELETED_FILE => parse_deleted_file(cursor, limits, edit, mutation_count)?,
        NEW_FILE | NEW_FILE2 | NEW_FILE3 | NEW_FILE4 => {
            parse_new_file_tag(tag, cursor, limits, edit, mutation_count)?;
        }
        COLUMN_FAMILY => {
            ensure_once(&mut seen.column_family, "column family selector")?;
            edit.column_family_id = cursor.read_varint_u32("column family selector")?;
        }
        COLUMN_FAMILY_ADD => {
            ensure_once(&mut seen.column_family_action, "column family add or drop")?;
            edit.column_family_action = Some(ColumnFamilyAction::Add {
                name: read_bytes(cursor, "column family name", limits)?,
            });
        }
        COLUMN_FAMILY_DROP => {
            ensure_once(&mut seen.column_family_action, "column family add or drop")?;
            edit.column_family_action = Some(ColumnFamilyAction::Drop);
        }
        IN_ATOMIC_GROUP => {
            ensure_once(&mut seen.atomic_group, "atomic group")?;
            edit.atomic_group_remaining =
                Some(cursor.read_varint_u32("atomic group remaining entries")?);
        }
        _ if tag & SAFE_IGNORE_MASK != 0 => {
            let field = cursor.read_length_prefixed(
                "safely ignorable VersionEdit field",
                limits.max_custom_field_bytes,
            )?;
            edit.ignored_fields.push(IgnoredField {
                tag,
                encoded_length: field.len() as u32,
            });
        }
        _ => return Err(RocksDbWireError::UnknownMandatoryTag { tag }),
    }
    Ok(())
}

fn parse_compact_cursor(
    cursor: &mut WireCursor<'_>,
    limits: VersionEditLimits,
    edit: &mut VersionEdit,
) -> Result<()> {
    if edit.compact_cursors.len() >= limits.max_compact_cursors {
        return Err(RocksDbWireError::CompactCursorLimit {
            limit: limits.max_compact_cursors,
        });
    }
    edit.compact_cursors.push(CompactCursor {
        level: parse_level(cursor, limits)?,
        key: parse_internal_key_metadata(cursor, "compact cursor key", limits)?,
    });
    Ok(())
}

fn parse_deleted_file(
    cursor: &mut WireCursor<'_>,
    limits: VersionEditLimits,
    edit: &mut VersionEdit,
    mutation_count: &mut usize,
) -> Result<()> {
    reserve_mutation(limits, mutation_count)?;
    let level = parse_level(cursor, limits)?;
    let file_number = cursor.read_varint_u64("deleted file number")?;
    validate_file_number(file_number)?;
    edit.deleted_files.push(DeletedFile { level, file_number });
    Ok(())
}

fn parse_new_file_tag(
    tag: u32,
    cursor: &mut WireCursor<'_>,
    limits: VersionEditLimits,
    edit: &mut VersionEdit,
    mutation_count: &mut usize,
) -> Result<()> {
    let format = match tag {
        NEW_FILE => NewFileFormat::NewFile,
        NEW_FILE2 => NewFileFormat::NewFile2,
        NEW_FILE3 => NewFileFormat::NewFile3,
        NEW_FILE4 => NewFileFormat::NewFile4,
        _ => return Err(RocksDbWireError::UnknownMandatoryTag { tag }),
    };
    let parsed = parse_new_file(format, cursor, limits, &mut edit.min_log_number_to_keep)?;
    if let Some(file) = parsed {
        reserve_mutation(limits, mutation_count)?;
        edit.new_files.push(file);
    }
    Ok(())
}

fn read_bytes(
    cursor: &mut WireCursor<'_>,
    context: &'static str,
    limits: VersionEditLimits,
) -> Result<Vec<u8>> {
    Ok(cursor
        .read_length_prefixed(context, limits.max_string_bytes)?
        .to_vec())
}

fn read_file_meta_number(
    cursor: &mut WireCursor<'_>,
    context: &'static str,
    allow_zero: bool,
) -> Result<u64> {
    let value = cursor.read_varint_u64(context)?;
    if value > FILE_NUMBER_MASK || (!allow_zero && value == 0) {
        return Err(RocksDbWireError::InvalidFileNumber { file_number: value });
    }
    Ok(value)
}

fn read_sequence(cursor: &mut WireCursor<'_>, context: &'static str) -> Result<u64> {
    let sequence = cursor.read_varint_u64(context)?;
    if sequence > MAX_SEQUENCE_NUMBER {
        return Err(RocksDbWireError::InvalidSequenceNumber { sequence });
    }
    Ok(sequence)
}

fn reserve_mutation(limits: VersionEditLimits, mutation_count: &mut usize) -> Result<()> {
    if *mutation_count >= limits.max_file_mutations {
        return Err(RocksDbWireError::FileMutationLimit {
            limit: limits.max_file_mutations,
        });
    }
    *mutation_count += 1;
    Ok(())
}

fn validate_file_number(file_number: u64) -> Result<()> {
    if file_number == 0 || file_number > FILE_NUMBER_MASK {
        return Err(RocksDbWireError::InvalidFileNumber { file_number });
    }
    Ok(())
}

fn ensure_once(seen: &mut bool, field: &'static str) -> Result<()> {
    if *seen {
        return Err(RocksDbWireError::DuplicateVersionEditField { field });
    }
    *seen = true;
    Ok(())
}

fn set_consistent_number(target: &mut Option<u64>, value: u64, field: &'static str) -> Result<()> {
    if let Some(existing) = *target {
        if existing != value {
            return Err(RocksDbWireError::ConflictingField {
                field,
                first: existing,
                second: value,
            });
        }
    } else {
        *target = Some(value);
    }
    Ok(())
}

fn validate_edit_shape(edit: &VersionEdit, mutation_count: usize) -> Result<()> {
    if edit.column_family_action.is_some() && mutation_count != 0 {
        return Err(RocksDbWireError::InvalidField {
            context: "column family manipulation",
            reason: "cannot include file mutations",
        });
    }
    Ok(())
}
