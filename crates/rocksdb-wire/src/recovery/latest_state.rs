use std::borrow::Cow;

use crate::{RocksDbWireError, ROCKSDB_MAX_SEQUENCE_NUMBER};

use super::{
    KeyVersion, KeyVersionKind, LatestState, LatestStateError, LatestStateLimits, LatestStateRef,
    MergeOperator,
};

const INTERNAL_TRAILER_BYTES: usize = 8;

/// Reduces one externally grouped user-key history.
///
/// `history` must be in strict descending RocksDB internal-trailer order. The
/// optional range sequence must already be known to cover `user_key`.
pub fn reduce_latest_state<M>(
    user_key: &[u8],
    history: &[KeyVersion<'_>],
    covering_range_tombstone_sequence: Option<u64>,
    limits: LatestStateLimits,
    merge_operator: &mut M,
) -> std::result::Result<Option<LatestState>, LatestStateError<M::Error>>
where
    M: MergeOperator,
{
    reduce_latest_state_ref(
        user_key,
        history,
        covering_range_tombstone_sequence,
        limits,
        merge_operator,
    )
    .map(|state| state.map(LatestStateRef::into_owned))
}

/// Reduces one externally grouped history while borrowing ordinary values.
///
/// Only resolved merge outputs allocate a new value buffer. Callers that only
/// inspect or hash the latest value can avoid cloning every non-merge value.
pub fn reduce_latest_state_ref<'a, M>(
    user_key: &[u8],
    history: &[KeyVersion<'a>],
    covering_range_tombstone_sequence: Option<u64>,
    limits: LatestStateLimits,
    merge_operator: &mut M,
) -> std::result::Result<Option<LatestStateRef<'a>>, LatestStateError<M::Error>>
where
    M: MergeOperator,
{
    validate_history(user_key, history, limits)?;
    validate_range_tombstone(covering_range_tombstone_sequence)?;

    let mut operands = Vec::new();
    let mut merge_sequence = None;
    let mut saw_hidden_point = false;

    for version in history {
        if is_hidden_by_range_tombstone(version.sequence, covering_range_tombstone_sequence) {
            saw_hidden_point = true;
            break;
        }
        match version.kind {
            KeyVersionKind::Value { value } => {
                if operands.is_empty() {
                    validate_resolved_value_length(value.len(), limits)?;
                    return Ok(Some(LatestStateRef::Value {
                        sequence: version.sequence,
                        value: Cow::Borrowed(value),
                    }));
                }
                return resolve_merge(
                    user_key,
                    required_merge_sequence(merge_sequence)?,
                    Some(value),
                    operands,
                    limits,
                    merge_operator,
                );
            }
            KeyVersionKind::Delete => {
                if operands.is_empty() {
                    return Ok(Some(LatestStateRef::Delete {
                        sequence: version.sequence,
                    }));
                }
                return resolve_merge(
                    user_key,
                    required_merge_sequence(merge_sequence)?,
                    None,
                    operands,
                    limits,
                    merge_operator,
                );
            }
            KeyVersionKind::SingleDelete => {
                if operands.is_empty() {
                    return Ok(Some(LatestStateRef::SingleDelete {
                        sequence: version.sequence,
                    }));
                }
                return resolve_merge(
                    user_key,
                    required_merge_sequence(merge_sequence)?,
                    None,
                    operands,
                    limits,
                    merge_operator,
                );
            }
            KeyVersionKind::Merge { operand } => {
                observe_merge_operand(&mut operands, operand, limits)?;
                merge_sequence.get_or_insert(version.sequence);
            }
        }
    }

    if !operands.is_empty() {
        return resolve_merge(
            user_key,
            required_merge_sequence(merge_sequence)?,
            None,
            operands,
            limits,
            merge_operator,
        );
    }
    if saw_hidden_point {
        return Ok(covering_range_tombstone_sequence
            .map(|sequence| LatestStateRef::RangeDelete { sequence }));
    }
    Ok(None)
}

fn validate_history(
    user_key: &[u8],
    history: &[KeyVersion<'_>],
    limits: LatestStateLimits,
) -> Result<(), RocksDbWireError> {
    if history.len() > limits.max_versions {
        return Err(RocksDbWireError::LatestStateVersionLimit {
            count: history.len(),
            limit: limits.max_versions,
        });
    }

    let internal_key_bytes = user_key.len().checked_add(INTERNAL_TRAILER_BYTES).ok_or(
        RocksDbWireError::LengthOverflow {
            context: "latest-state internal key length",
        },
    )?;
    let mut history_bytes = 0usize;
    let mut previous_trailer = None;
    for version in history {
        validate_sequence(version.sequence)?;
        let trailer = (version.sequence << 8) | u64::from(version.kind.value_type());
        validate_trailer_order(previous_trailer, trailer)?;
        previous_trailer = Some(trailer);

        let version_bytes = internal_key_bytes
            .checked_add(version.kind.payload_length())
            .ok_or(RocksDbWireError::LengthOverflow {
                context: "latest-state key history bytes",
            })?;
        history_bytes =
            history_bytes
                .checked_add(version_bytes)
                .ok_or(RocksDbWireError::LengthOverflow {
                    context: "latest-state key history bytes",
                })?;
        if history_bytes > limits.max_key_history_bytes {
            return Err(RocksDbWireError::LatestStateHistoryBytesLimit {
                bytes: history_bytes,
                limit: limits.max_key_history_bytes,
            });
        }
    }
    Ok(())
}

fn validate_trailer_order(previous: Option<u64>, current: u64) -> Result<(), RocksDbWireError> {
    let Some(previous) = previous else {
        return Ok(());
    };
    if previous == current {
        return Err(RocksDbWireError::DuplicateLatestStateInternalKey { trailer: current });
    }
    if previous < current {
        return Err(RocksDbWireError::LatestStateHistoryOutOfOrder {
            previous_trailer: previous,
            current_trailer: current,
        });
    }
    Ok(())
}

fn validate_range_tombstone(sequence: Option<u64>) -> Result<(), RocksDbWireError> {
    if let Some(sequence) = sequence {
        validate_sequence(sequence)?;
    }
    Ok(())
}

fn required_merge_sequence(sequence: Option<u64>) -> Result<u64, RocksDbWireError> {
    sequence.ok_or(RocksDbWireError::InvalidField {
        context: "latest-state merge history",
        reason: "merge operands have no leading sequence",
    })
}

fn validate_sequence(sequence: u64) -> Result<(), RocksDbWireError> {
    if sequence > ROCKSDB_MAX_SEQUENCE_NUMBER {
        return Err(RocksDbWireError::InvalidSequenceNumber { sequence });
    }
    Ok(())
}

fn is_hidden_by_range_tombstone(point_sequence: u64, tombstone_sequence: Option<u64>) -> bool {
    tombstone_sequence.is_some_and(|sequence| sequence > point_sequence)
}

fn observe_merge_operand<'a>(
    operands: &mut Vec<&'a [u8]>,
    operand: &'a [u8],
    limits: LatestStateLimits,
) -> Result<(), RocksDbWireError> {
    let count = operands
        .len()
        .checked_add(1)
        .ok_or(RocksDbWireError::LengthOverflow {
            context: "latest-state merge operand count",
        })?;
    if count > limits.max_merge_operands {
        return Err(RocksDbWireError::LatestStateMergeOperandLimit {
            count,
            limit: limits.max_merge_operands,
        });
    }
    operands.push(operand);
    Ok(())
}

fn resolve_merge<'a, M>(
    user_key: &[u8],
    sequence: u64,
    existing_value: Option<&[u8]>,
    mut operands: Vec<&[u8]>,
    limits: LatestStateLimits,
    merge_operator: &mut M,
) -> std::result::Result<Option<LatestStateRef<'a>>, LatestStateError<M::Error>>
where
    M: MergeOperator,
{
    operands.reverse();
    let value = merge_operator
        .full_merge(
            user_key,
            existing_value,
            &operands,
            limits.max_resolved_value_bytes,
        )
        .map_err(LatestStateError::MergeOperator)?;
    validate_resolved_value_length(value.len(), limits)?;
    Ok(Some(LatestStateRef::Value {
        sequence,
        value: Cow::Owned(value),
    }))
}

fn validate_resolved_value_length(
    length: usize,
    limits: LatestStateLimits,
) -> Result<(), RocksDbWireError> {
    if length > limits.max_resolved_value_bytes {
        return Err(RocksDbWireError::LatestStateResolvedValueLimit {
            length,
            limit: limits.max_resolved_value_bytes,
        });
    }
    Ok(())
}
