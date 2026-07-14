use std::borrow::Cow;

use rocksdb_wire::{
    reduce_latest_state, reduce_latest_state_ref, KeyVersion, LatestState, LatestStateError,
    LatestStateLimits, LatestStateRef, MergeOperator, RocksDbWireError,
    ROCKSDB_MAX_SEQUENCE_NUMBER,
};

#[derive(Debug, PartialEq, Eq)]
struct MergeCall {
    user_key: Vec<u8>,
    existing_value: Option<Vec<u8>>,
    operands: Vec<Vec<u8>>,
    max_output_bytes: usize,
}

#[derive(Default)]
struct RecordingMerge {
    calls: Vec<MergeCall>,
}

impl MergeOperator for RecordingMerge {
    type Error = &'static str;

    fn full_merge(
        &mut self,
        user_key: &[u8],
        existing_value: Option<&[u8]>,
        operands_oldest_to_newest: &[&[u8]],
        max_output_bytes: usize,
    ) -> Result<Vec<u8>, Self::Error> {
        self.calls.push(MergeCall {
            user_key: user_key.to_vec(),
            existing_value: existing_value.map(<[u8]>::to_vec),
            operands: operands_oldest_to_newest
                .iter()
                .map(|operand| operand.to_vec())
                .collect(),
            max_output_bytes,
        });
        let mut resolved = existing_value.unwrap_or_default().to_vec();
        for operand in operands_oldest_to_newest {
            resolved.extend_from_slice(operand);
        }
        Ok(resolved)
    }
}

fn limits() -> LatestStateLimits {
    LatestStateLimits {
        max_versions: 16,
        max_key_history_bytes: 1024,
        max_merge_operands: 8,
        max_resolved_value_bytes: 1024,
    }
}

#[test]
fn value_delete_and_single_delete_produce_distinct_states() {
    let mut merge = RecordingMerge::default();

    let value = reduce_latest_state(
        b"key",
        &[KeyVersion::value(9, b"value")],
        None,
        limits(),
        &mut merge,
    )
    .expect("value history should reduce");
    let delete = reduce_latest_state(b"key", &[KeyVersion::delete(9)], None, limits(), &mut merge)
        .expect("delete history should reduce");
    let single_delete = reduce_latest_state(
        b"key",
        &[KeyVersion::single_delete(9)],
        None,
        limits(),
        &mut merge,
    )
    .expect("single-delete history should reduce");

    assert_eq!(
        value,
        Some(LatestState::Value {
            sequence: 9,
            value: b"value".to_vec(),
        })
    );
    assert_eq!(delete, Some(LatestState::Delete { sequence: 9 }));
    assert_eq!(
        single_delete,
        Some(LatestState::SingleDelete { sequence: 9 })
    );
    assert!(merge.calls.is_empty());
}

#[test]
fn borrowed_reducer_only_allocates_resolved_merge_values() {
    let mut merge = RecordingMerge::default();
    let direct = reduce_latest_state_ref(
        b"key",
        &[KeyVersion::value(9, b"value")],
        None,
        limits(),
        &mut merge,
    )
    .expect("borrowed value history should reduce");
    assert!(matches!(
        direct,
        Some(LatestStateRef::Value {
            sequence: 9,
            value: Cow::Borrowed(b"value"),
        })
    ));

    let merged = reduce_latest_state_ref(
        b"key",
        &[
            KeyVersion::merge(10, b"suffix"),
            KeyVersion::value(9, b"base"),
        ],
        None,
        limits(),
        &mut merge,
    )
    .expect("borrowed merge history should reduce");
    assert!(matches!(
        merged,
        Some(LatestStateRef::Value {
            sequence: 10,
            value: Cow::Owned(ref value),
        }) if value == b"basesuffix"
    ));
}

#[test]
fn merge_operands_are_reversed_to_oldest_first() {
    let history = [
        KeyVersion::merge(30, b"new"),
        KeyVersion::merge(20, b"old"),
        KeyVersion::value(10, b"base"),
    ];
    let mut merge = RecordingMerge::default();

    let state = reduce_latest_state(b"key", &history, None, limits(), &mut merge)
        .expect("merge history should reduce");

    assert_eq!(
        state,
        Some(LatestState::Value {
            sequence: 30,
            value: b"baseoldnew".to_vec(),
        })
    );
    assert_eq!(
        merge.calls,
        vec![MergeCall {
            user_key: b"key".to_vec(),
            existing_value: Some(b"base".to_vec()),
            operands: vec![b"old".to_vec(), b"new".to_vec()],
            max_output_bytes: 1024,
        }]
    );
}

#[test]
fn range_tombstone_only_hides_strictly_older_points() {
    let history = [KeyVersion::value(10, b"value")];
    let mut merge = RecordingMerge::default();

    let equal = reduce_latest_state(b"key", &history, Some(10), limits(), &mut merge)
        .expect("equal-sequence tombstone must not hide the point");
    let older = reduce_latest_state(b"key", &history, Some(9), limits(), &mut merge)
        .expect("older tombstone must not hide the point");
    let newer = reduce_latest_state(b"key", &history, Some(11), limits(), &mut merge)
        .expect("newer tombstone should reduce to range delete");

    let expected_value = Some(LatestState::Value {
        sequence: 10,
        value: b"value".to_vec(),
    });
    assert_eq!(equal, expected_value);
    assert_eq!(older, expected_value);
    assert_eq!(newer, Some(LatestState::RangeDelete { sequence: 11 }));
}

#[test]
fn range_tombstone_can_hide_a_merge_base_without_hiding_newer_operands() {
    let history = [
        KeyVersion::merge(30, b"new"),
        KeyVersion::merge(20, b"old"),
        KeyVersion::value(10, b"base"),
    ];
    let mut merge = RecordingMerge::default();

    let state = reduce_latest_state(b"key", &history, Some(15), limits(), &mut merge)
        .expect("surviving merge operands should resolve without the hidden base");

    assert_eq!(
        state,
        Some(LatestState::Value {
            sequence: 30,
            value: b"oldnew".to_vec(),
        })
    );
    assert_eq!(merge.calls[0].existing_value, None);
    assert_eq!(
        merge.calls[0].operands,
        vec![b"old".to_vec(), b"new".to_vec()]
    );
}

#[test]
fn duplicate_internal_key_fails_closed() {
    let history = [
        KeyVersion::value(7, b"first"),
        KeyVersion::value(7, b"duplicate"),
    ];
    let mut merge = RecordingMerge::default();

    let error = reduce_latest_state(b"key", &history, None, limits(), &mut merge)
        .expect_err("duplicate trailer must fail");

    assert_eq!(
        error,
        LatestStateError::Wire(RocksDbWireError::DuplicateLatestStateInternalKey {
            trailer: (7 << 8) | 0x01,
        })
    );
    assert!(merge.calls.is_empty());
}

#[test]
fn non_descending_internal_trailer_fails_closed() {
    let history = [
        KeyVersion::value(7, b"older"),
        KeyVersion::merge(8, b"newer"),
    ];
    let mut merge = RecordingMerge::default();

    let error = reduce_latest_state(b"key", &history, None, limits(), &mut merge)
        .expect_err("sequence regression must fail");

    assert_eq!(
        error,
        LatestStateError::Wire(RocksDbWireError::LatestStateHistoryOutOfOrder {
            previous_trailer: (7 << 8) | 0x01,
            current_trailer: (8 << 8) | 0x02,
        })
    );
    assert!(merge.calls.is_empty());
}

#[test]
fn same_sequence_uses_value_type_to_define_strict_order() {
    let history = [
        KeyVersion::single_delete(5),
        KeyVersion::merge(5, b"operand"),
        KeyVersion::value(5, b"value"),
        KeyVersion::delete(5),
    ];
    let mut merge = RecordingMerge::default();

    let state = reduce_latest_state(b"key", &history, None, limits(), &mut merge)
        .expect("strictly descending value types at one sequence are valid");

    assert_eq!(state, Some(LatestState::SingleDelete { sequence: 5 }));
    assert!(merge.calls.is_empty());
}

#[test]
fn sequence_numbers_are_bounded_before_reduction() {
    let history = [KeyVersion::value(ROCKSDB_MAX_SEQUENCE_NUMBER + 1, b"value")];
    let mut merge = RecordingMerge::default();

    let error = reduce_latest_state(b"key", &history, None, limits(), &mut merge)
        .expect_err("out-of-range point sequence must fail");
    assert_eq!(
        error,
        LatestStateError::Wire(RocksDbWireError::InvalidSequenceNumber {
            sequence: ROCKSDB_MAX_SEQUENCE_NUMBER + 1,
        })
    );

    let error = reduce_latest_state(
        b"key",
        &[KeyVersion::value(1, b"value")],
        Some(ROCKSDB_MAX_SEQUENCE_NUMBER + 1),
        limits(),
        &mut merge,
    )
    .expect_err("out-of-range tombstone sequence must fail");
    assert_eq!(
        error,
        LatestStateError::Wire(RocksDbWireError::InvalidSequenceNumber {
            sequence: ROCKSDB_MAX_SEQUENCE_NUMBER + 1,
        })
    );
}

#[test]
fn every_latest_state_limit_is_enforced() {
    let history = [
        KeyVersion::merge(3, b"a"),
        KeyVersion::merge(2, b"b"),
        KeyVersion::value(1, b"base"),
    ];
    let mut merge = RecordingMerge::default();

    let error = reduce_latest_state(
        b"k",
        &history,
        None,
        LatestStateLimits {
            max_versions: 2,
            ..limits()
        },
        &mut merge,
    )
    .expect_err("version limit must fail");
    assert_eq!(
        error,
        LatestStateError::Wire(RocksDbWireError::LatestStateVersionLimit { count: 3, limit: 2 })
    );

    let error = reduce_latest_state(
        b"k",
        &[KeyVersion::value(1, b"v")],
        None,
        LatestStateLimits {
            max_key_history_bytes: 9,
            ..limits()
        },
        &mut merge,
    )
    .expect_err("history byte limit must count user key, trailer, and payload");
    assert_eq!(
        error,
        LatestStateError::Wire(RocksDbWireError::LatestStateHistoryBytesLimit {
            bytes: 10,
            limit: 9,
        })
    );

    let error = reduce_latest_state(
        b"k",
        &history,
        None,
        LatestStateLimits {
            max_merge_operands: 1,
            ..limits()
        },
        &mut merge,
    )
    .expect_err("merge operand limit must fail");
    assert_eq!(
        error,
        LatestStateError::Wire(RocksDbWireError::LatestStateMergeOperandLimit {
            count: 2,
            limit: 1,
        })
    );

    let error = reduce_latest_state(
        b"k",
        &[KeyVersion::value(1, b"too long")],
        None,
        LatestStateLimits {
            max_resolved_value_bytes: 3,
            ..limits()
        },
        &mut merge,
    )
    .expect_err("direct resolved value limit must fail");
    assert_eq!(
        error,
        LatestStateError::Wire(RocksDbWireError::LatestStateResolvedValueLimit {
            length: 8,
            limit: 3,
        })
    );

    let error = reduce_latest_state(
        b"k",
        &history,
        None,
        LatestStateLimits {
            max_resolved_value_bytes: 2,
            ..limits()
        },
        &mut merge,
    )
    .expect_err("merged resolved value limit must fail");
    assert_eq!(
        error,
        LatestStateError::Wire(RocksDbWireError::LatestStateResolvedValueLimit {
            length: 6,
            limit: 2,
        })
    );
}

#[test]
fn exact_history_byte_limit_and_empty_history_are_valid_edges() {
    let mut merge = RecordingMerge::default();
    let state = reduce_latest_state(
        b"k",
        &[KeyVersion::value(1, b"v")],
        None,
        LatestStateLimits {
            max_key_history_bytes: 10,
            ..limits()
        },
        &mut merge,
    )
    .expect("exact history byte limit should be accepted");
    assert_eq!(
        state,
        Some(LatestState::Value {
            sequence: 1,
            value: b"v".to_vec(),
        })
    );

    let empty = reduce_latest_state(b"k", &[], Some(10), limits(), &mut merge)
        .expect("an empty point history has no per-key state");
    assert_eq!(empty, None);
}

#[test]
fn merge_operator_error_is_preserved() {
    struct FailingMerge;

    impl MergeOperator for FailingMerge {
        type Error = &'static str;

        fn full_merge(
            &mut self,
            _user_key: &[u8],
            _existing_value: Option<&[u8]>,
            _operands_oldest_to_newest: &[&[u8]],
            _max_output_bytes: usize,
        ) -> Result<Vec<u8>, Self::Error> {
            Err("semantic decoder rejected operand")
        }
    }

    let mut merge = FailingMerge;
    let error = reduce_latest_state(
        b"key",
        &[KeyVersion::merge(1, b"operand")],
        None,
        limits(),
        &mut merge,
    )
    .expect_err("merge operator failure should be returned without stringification");

    assert_eq!(
        error,
        LatestStateError::MergeOperator("semantic decoder rejected operand")
    );
}
