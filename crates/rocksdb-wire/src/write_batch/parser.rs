use crate::cursor::WireCursor;
use crate::{Result, RocksDbWireError, WriteBatchLimits};

use super::model::{
    WriteBatch, WriteBatchAuxiliaryKind, WriteBatchAuxiliaryRecord, WriteBatchMutation,
    WriteBatchMutationKind, ROCKSDB_MAX_SEQUENCE_NUMBER,
};

const TYPE_DELETION: u8 = 0x00;
const TYPE_VALUE: u8 = 0x01;
const TYPE_MERGE: u8 = 0x02;
const TYPE_LOG_DATA: u8 = 0x03;
const TYPE_COLUMN_FAMILY_DELETION: u8 = 0x04;
const TYPE_COLUMN_FAMILY_VALUE: u8 = 0x05;
const TYPE_COLUMN_FAMILY_MERGE: u8 = 0x06;
const TYPE_SINGLE_DELETION: u8 = 0x07;
const TYPE_COLUMN_FAMILY_SINGLE_DELETION: u8 = 0x08;
const TYPE_BEGIN_PREPARE_XID: u8 = 0x09;
const TYPE_END_PREPARE_XID: u8 = 0x0a;
const TYPE_COMMIT_XID: u8 = 0x0b;
const TYPE_ROLLBACK_XID: u8 = 0x0c;
const TYPE_NOOP: u8 = 0x0d;
const TYPE_COLUMN_FAMILY_RANGE_DELETION: u8 = 0x0e;
const TYPE_RANGE_DELETION: u8 = 0x0f;
const TYPE_COLUMN_FAMILY_BLOB_INDEX: u8 = 0x10;
const TYPE_BLOB_INDEX: u8 = 0x11;
const TYPE_BEGIN_PERSISTED_PREPARE_XID: u8 = 0x12;
const TYPE_BEGIN_UNPREPARE_XID: u8 = 0x13;
const TYPE_DELETION_WITH_TIMESTAMP: u8 = 0x14;
const TYPE_COMMIT_XID_AND_TIMESTAMP: u8 = 0x15;
const TYPE_WIDE_COLUMN_ENTITY: u8 = 0x16;
const TYPE_COLUMN_FAMILY_WIDE_COLUMN_ENTITY: u8 = 0x17;

pub fn decode_write_batch(input: &[u8], limits: WriteBatchLimits) -> Result<WriteBatch<'_>> {
    validate_batch_length(input, limits)?;
    let mut cursor = WireCursor::new(input);
    let sequence = cursor.read_fixed_u64("WriteBatch sequence")?;
    validate_sequence(sequence)?;
    let declared_count = cursor.read_fixed_u32("WriteBatch mutation count")?;
    validate_declared_count(declared_count, limits)?;
    validate_last_sequence(sequence, declared_count)?;

    let mut mutations = Vec::with_capacity((declared_count as usize).min(4096));
    let mut auxiliary_records = Vec::new();
    while !cursor.is_empty() {
        decode_record(
            &mut cursor,
            &mut mutations,
            sequence,
            limits,
            &mut auxiliary_records,
        )?;
        validate_decoded_count(mutations.len(), limits)?;
    }

    validate_count_match(mutations.len(), declared_count, limits)?;
    let auxiliary_record_count = u32::try_from(auxiliary_records.len()).map_err(|_| {
        RocksDbWireError::WriteBatchAuxiliaryRecordLimit {
            limit: limits.max_auxiliary_records,
        }
    })?;
    Ok(WriteBatch {
        sequence,
        declared_count,
        auxiliary_record_count,
        auxiliary_records,
        mutations,
    })
}

fn decode_record<'a>(
    cursor: &mut WireCursor<'a>,
    mutations: &mut Vec<WriteBatchMutation<'a>>,
    sequence: u64,
    limits: WriteBatchLimits,
    auxiliary_records: &mut Vec<WriteBatchAuxiliaryRecord<'a>>,
) -> Result<()> {
    let tag_offset = cursor.position();
    let tag = cursor.read_u8("WriteBatch tag")?;
    match tag {
        TYPE_DELETION => push_key_mutation(
            cursor,
            mutations,
            sequence,
            0,
            limits,
            WriteBatchMutationKind::Delete,
        ),
        TYPE_VALUE => push_key_value_mutation(
            cursor,
            mutations,
            sequence,
            0,
            limits,
            MutationWithValue::Put,
        ),
        TYPE_MERGE => push_key_value_mutation(
            cursor,
            mutations,
            sequence,
            0,
            limits,
            MutationWithValue::Merge,
        ),
        TYPE_LOG_DATA => {
            let data =
                cursor.read_length_prefixed("WriteBatch log data", limits.max_value_bytes)?;
            push_auxiliary(
                auxiliary_records,
                limits,
                tag_offset,
                WriteBatchAuxiliaryKind::LogData { data },
            )
        }
        TYPE_SINGLE_DELETION => push_key_mutation(
            cursor,
            mutations,
            sequence,
            0,
            limits,
            WriteBatchMutationKind::SingleDelete,
        ),
        TYPE_NOOP => push_auxiliary(
            auxiliary_records,
            limits,
            tag_offset,
            WriteBatchAuxiliaryKind::Noop,
        ),
        TYPE_RANGE_DELETION => push_range_delete(cursor, mutations, sequence, 0, limits),
        TYPE_COLUMN_FAMILY_DELETION
        | TYPE_COLUMN_FAMILY_VALUE
        | TYPE_COLUMN_FAMILY_MERGE
        | TYPE_COLUMN_FAMILY_SINGLE_DELETION
        | TYPE_COLUMN_FAMILY_RANGE_DELETION => {
            decode_column_family_record(cursor, mutations, sequence, limits, tag)
        }
        _ if is_unsupported_tag(tag) => Err(RocksDbWireError::UnsupportedWriteBatchTag {
            offset: tag_offset,
            tag,
        }),
        _ => Err(RocksDbWireError::InvalidWriteBatchTag {
            offset: tag_offset,
            tag,
        }),
    }
}

fn decode_column_family_record<'a>(
    cursor: &mut WireCursor<'a>,
    mutations: &mut Vec<WriteBatchMutation<'a>>,
    sequence: u64,
    limits: WriteBatchLimits,
    tag: u8,
) -> Result<()> {
    let column_family_id = cursor.read_varint_u32("WriteBatch column family ID")?;
    match tag {
        TYPE_COLUMN_FAMILY_DELETION => push_key_mutation(
            cursor,
            mutations,
            sequence,
            column_family_id,
            limits,
            WriteBatchMutationKind::Delete,
        ),
        TYPE_COLUMN_FAMILY_VALUE => push_key_value_mutation(
            cursor,
            mutations,
            sequence,
            column_family_id,
            limits,
            MutationWithValue::Put,
        ),
        TYPE_COLUMN_FAMILY_MERGE => push_key_value_mutation(
            cursor,
            mutations,
            sequence,
            column_family_id,
            limits,
            MutationWithValue::Merge,
        ),
        TYPE_COLUMN_FAMILY_SINGLE_DELETION => push_key_mutation(
            cursor,
            mutations,
            sequence,
            column_family_id,
            limits,
            WriteBatchMutationKind::SingleDelete,
        ),
        TYPE_COLUMN_FAMILY_RANGE_DELETION => {
            push_range_delete(cursor, mutations, sequence, column_family_id, limits)
        }
        _ => Err(RocksDbWireError::InvalidWriteBatchTag {
            offset: cursor.position().saturating_sub(1),
            tag,
        }),
    }
}

fn is_unsupported_tag(tag: u8) -> bool {
    matches!(
        tag,
        TYPE_BEGIN_PREPARE_XID
            | TYPE_END_PREPARE_XID
            | TYPE_COMMIT_XID
            | TYPE_ROLLBACK_XID
            | TYPE_COLUMN_FAMILY_BLOB_INDEX
            | TYPE_BLOB_INDEX
            | TYPE_BEGIN_PERSISTED_PREPARE_XID
            | TYPE_BEGIN_UNPREPARE_XID
            | TYPE_DELETION_WITH_TIMESTAMP
            | TYPE_COMMIT_XID_AND_TIMESTAMP
            | TYPE_WIDE_COLUMN_ENTITY
            | TYPE_COLUMN_FAMILY_WIDE_COLUMN_ENTITY
    )
}

fn validate_batch_length(input: &[u8], limits: WriteBatchLimits) -> Result<()> {
    if input.len() > limits.max_batch_bytes {
        return Err(RocksDbWireError::WriteBatchLengthLimit {
            length: input.len(),
            limit: limits.max_batch_bytes,
        });
    }
    Ok(())
}

fn validate_declared_count(count: u32, limits: WriteBatchLimits) -> Result<()> {
    if count as usize > limits.max_mutations {
        return Err(RocksDbWireError::WriteBatchMutationLimit {
            count,
            limit: limits.max_mutations,
        });
    }
    Ok(())
}

fn validate_decoded_count(count: usize, limits: WriteBatchLimits) -> Result<()> {
    if count > limits.max_mutations {
        return Err(RocksDbWireError::WriteBatchMutationLimit {
            count: count.try_into().unwrap_or(u32::MAX),
            limit: limits.max_mutations,
        });
    }
    Ok(())
}

fn validate_count_match(count: usize, declared: u32, limits: WriteBatchLimits) -> Result<()> {
    let decoded = u32::try_from(count).map_err(|_| RocksDbWireError::WriteBatchMutationLimit {
        count: u32::MAX,
        limit: limits.max_mutations,
    })?;
    if decoded != declared {
        return Err(RocksDbWireError::WriteBatchCountMismatch { declared, decoded });
    }
    Ok(())
}

fn push_key_mutation<'a>(
    cursor: &mut WireCursor<'a>,
    mutations: &mut Vec<WriteBatchMutation<'a>>,
    batch_sequence: u64,
    column_family_id: u32,
    limits: WriteBatchLimits,
    kind: WriteBatchMutationKind<'a>,
) -> Result<()> {
    let key = cursor.read_length_prefixed("WriteBatch key", limits.max_key_bytes)?;
    push_mutation(mutations, batch_sequence, column_family_id, key, kind)
}

enum MutationWithValue {
    Put,
    Merge,
}

fn push_key_value_mutation<'a>(
    cursor: &mut WireCursor<'a>,
    mutations: &mut Vec<WriteBatchMutation<'a>>,
    batch_sequence: u64,
    column_family_id: u32,
    limits: WriteBatchLimits,
    kind: MutationWithValue,
) -> Result<()> {
    let key = cursor.read_length_prefixed("WriteBatch key", limits.max_key_bytes)?;
    let value = cursor.read_length_prefixed("WriteBatch value", limits.max_value_bytes)?;
    let kind = match kind {
        MutationWithValue::Put => WriteBatchMutationKind::Put { value },
        MutationWithValue::Merge => WriteBatchMutationKind::Merge { operand: value },
    };
    push_mutation(mutations, batch_sequence, column_family_id, key, kind)
}

fn push_range_delete<'a>(
    cursor: &mut WireCursor<'a>,
    mutations: &mut Vec<WriteBatchMutation<'a>>,
    batch_sequence: u64,
    column_family_id: u32,
    limits: WriteBatchLimits,
) -> Result<()> {
    let key = cursor.read_length_prefixed("WriteBatch range start", limits.max_key_bytes)?;
    let end_key = cursor.read_length_prefixed("WriteBatch range end", limits.max_key_bytes)?;
    push_mutation(
        mutations,
        batch_sequence,
        column_family_id,
        key,
        WriteBatchMutationKind::DeleteRange { end_key },
    )
}

fn push_mutation<'a>(
    mutations: &mut Vec<WriteBatchMutation<'a>>,
    batch_sequence: u64,
    column_family_id: u32,
    key: &'a [u8],
    kind: WriteBatchMutationKind<'a>,
) -> Result<()> {
    let ordinal = mutations.len() as u64;
    let sequence = batch_sequence
        .checked_add(ordinal)
        .ok_or(RocksDbWireError::LengthOverflow {
            context: "WriteBatch mutation sequence",
        })?;
    validate_sequence(sequence)?;
    mutations.push(WriteBatchMutation {
        sequence,
        column_family_id,
        key,
        kind,
    });
    Ok(())
}

fn push_auxiliary<'a>(
    records: &mut Vec<WriteBatchAuxiliaryRecord<'a>>,
    limits: WriteBatchLimits,
    offset: usize,
    kind: WriteBatchAuxiliaryKind<'a>,
) -> Result<()> {
    if records.len() >= limits.max_auxiliary_records {
        return Err(RocksDbWireError::WriteBatchAuxiliaryRecordLimit {
            limit: limits.max_auxiliary_records,
        });
    }
    records.push(WriteBatchAuxiliaryRecord { offset, kind });
    Ok(())
}

fn validate_last_sequence(sequence: u64, count: u32) -> Result<()> {
    if count == 0 {
        return Ok(());
    }
    let last =
        sequence
            .checked_add(u64::from(count) - 1)
            .ok_or(RocksDbWireError::LengthOverflow {
                context: "WriteBatch last sequence",
            })?;
    validate_sequence(last)
}

fn validate_sequence(sequence: u64) -> Result<()> {
    if sequence > ROCKSDB_MAX_SEQUENCE_NUMBER {
        return Err(RocksDbWireError::InvalidSequenceNumber { sequence });
    }
    Ok(())
}
