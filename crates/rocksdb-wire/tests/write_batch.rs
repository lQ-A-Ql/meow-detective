use rocksdb_wire::{
    decode_write_batch, RocksDbWireError, WriteBatchAuxiliaryKind, WriteBatchLimits,
    WriteBatchMutationKind, ROCKSDB_MAX_SEQUENCE_NUMBER, WRITE_BATCH_HEADER_SIZE,
};

const DELETION: u8 = 0x00;
const VALUE: u8 = 0x01;
const MERGE: u8 = 0x02;
const LOG_DATA: u8 = 0x03;
const CF_DELETION: u8 = 0x04;
const CF_VALUE: u8 = 0x05;
const CF_MERGE: u8 = 0x06;
const SINGLE_DELETION: u8 = 0x07;
const CF_SINGLE_DELETION: u8 = 0x08;
const NOOP: u8 = 0x0d;
const CF_RANGE_DELETION: u8 = 0x0e;
const RANGE_DELETION: u8 = 0x0f;

#[test]
fn decodes_supported_default_and_column_family_mutations() {
    let mut records = Vec::new();
    key_record(DELETION, b"delete", &mut records);
    key_value_record(VALUE, b"put", b"value", &mut records);
    key_value_record(MERGE, b"merge", b"operand", &mut records);
    key_record(SINGLE_DELETION, b"single", &mut records);
    range_record(RANGE_DELETION, b"a", b"z", &mut records);
    cf_key_record(CF_DELETION, 4, b"cf-delete", &mut records);
    cf_key_value_record(CF_VALUE, 5, b"cf-put", b"cf-value", &mut records);
    cf_key_value_record(CF_MERGE, 6, b"cf-merge", b"cf-operand", &mut records);
    cf_key_record(CF_SINGLE_DELETION, 7, b"cf-single", &mut records);
    cf_range_record(CF_RANGE_DELETION, 8, b"cf-a", b"cf-z", &mut records);
    let bytes = batch(100, 10, records);

    let decoded =
        decode_write_batch(&bytes, WriteBatchLimits::default()).expect("decode WriteBatch");

    assert_eq!(decoded.sequence, 100);
    assert_eq!(decoded.declared_count, 10);
    assert_eq!(decoded.last_sequence(), Some(109));
    assert_eq!(decoded.auxiliary_record_count, 0);
    assert_eq!(decoded.mutations.len(), 10);
    assert_eq!(decoded.mutations[0].sequence, 100);
    assert_eq!(decoded.mutations[0].column_family_id, 0);
    assert_eq!(decoded.mutations[0].key, b"delete");
    assert_eq!(decoded.mutations[0].kind, WriteBatchMutationKind::Delete);
    assert_eq!(
        decoded.mutations[1].kind,
        WriteBatchMutationKind::Put { value: b"value" }
    );
    assert_eq!(
        decoded.mutations[2].kind,
        WriteBatchMutationKind::Merge {
            operand: b"operand"
        }
    );
    assert_eq!(
        decoded.mutations[3].kind,
        WriteBatchMutationKind::SingleDelete
    );
    assert_eq!(
        decoded.mutations[4].kind,
        WriteBatchMutationKind::DeleteRange { end_key: b"z" }
    );
    assert_eq!(decoded.mutations[5].column_family_id, 4);
    assert_eq!(decoded.mutations[9].column_family_id, 8);
    assert_eq!(decoded.mutations[9].sequence, 109);
}

#[test]
fn auxiliary_records_do_not_change_declared_count_or_sequences() {
    let mut records = Vec::new();
    records.push(NOOP);
    records.push(LOG_DATA);
    length_prefixed(b"opaque", &mut records);
    key_value_record(VALUE, b"key", b"value", &mut records);
    records.push(NOOP);
    let bytes = batch(77, 1, records);

    let decoded =
        decode_write_batch(&bytes, WriteBatchLimits::default()).expect("decode auxiliaries");
    assert_eq!(decoded.auxiliary_record_count, 3);
    assert_eq!(decoded.auxiliary_records.len(), 3);
    assert_eq!(decoded.auxiliary_records[0].offset, WRITE_BATCH_HEADER_SIZE);
    assert_eq!(
        decoded.auxiliary_records[0].kind,
        WriteBatchAuxiliaryKind::Noop
    );
    assert!(matches!(
        decoded.auxiliary_records[1].kind,
        WriteBatchAuxiliaryKind::LogData { data: b"opaque" }
    ));
    assert_eq!(
        decoded.auxiliary_records[2].kind,
        WriteBatchAuxiliaryKind::Noop
    );
    assert_eq!(decoded.mutations.len(), 1);
    assert_eq!(decoded.mutations[0].sequence, 77);
    assert_eq!(decoded.last_sequence(), Some(77));
}

#[test]
fn returns_borrowed_key_and_value_slices() {
    let mut records = Vec::new();
    key_value_record(VALUE, b"key", b"value", &mut records);
    let bytes = batch(5, 1, records);
    let start = bytes.as_ptr() as usize;
    let end = start + bytes.len();

    let decoded =
        decode_write_batch(&bytes, WriteBatchLimits::default()).expect("decode borrowed slices");
    let key = decoded.mutations[0].key.as_ptr() as usize;
    let value = match decoded.mutations[0].kind {
        WriteBatchMutationKind::Put { value } => value.as_ptr() as usize,
        _ => panic!("expected put"),
    };
    assert!((start..end).contains(&key));
    assert!((start..end).contains(&value));
}

#[test]
fn accepts_empty_batches_and_maximum_sequence() {
    let empty = batch(ROCKSDB_MAX_SEQUENCE_NUMBER, 0, Vec::new());
    let decoded =
        decode_write_batch(&empty, WriteBatchLimits::default()).expect("decode empty batch");
    assert!(decoded.mutations.is_empty());
    assert_eq!(decoded.last_sequence(), None);

    let mut records = Vec::new();
    key_record(DELETION, b"last", &mut records);
    let last = batch(ROCKSDB_MAX_SEQUENCE_NUMBER, 1, records);
    let decoded =
        decode_write_batch(&last, WriteBatchLimits::default()).expect("decode final sequence");
    assert_eq!(decoded.last_sequence(), Some(ROCKSDB_MAX_SEQUENCE_NUMBER));
}

#[test]
fn rejects_short_headers_count_mismatch_and_sequence_overflow() {
    for length in 0..WRITE_BATCH_HEADER_SIZE {
        assert!(matches!(
            decode_write_batch(&vec![0; length], WriteBatchLimits::default()),
            Err(RocksDbWireError::UnexpectedEof { .. })
        ));
    }

    let mut records = Vec::new();
    key_record(DELETION, b"one", &mut records);
    assert!(matches!(
        decode_write_batch(&batch(1, 2, records.clone()), WriteBatchLimits::default()),
        Err(RocksDbWireError::WriteBatchCountMismatch {
            declared: 2,
            decoded: 1
        })
    ));
    assert!(matches!(
        decode_write_batch(&batch(1, 0, records), WriteBatchLimits::default()),
        Err(RocksDbWireError::WriteBatchCountMismatch {
            declared: 0,
            decoded: 1
        })
    ));
    assert!(matches!(
        decode_write_batch(
            &batch(ROCKSDB_MAX_SEQUENCE_NUMBER + 1, 0, Vec::new()),
            WriteBatchLimits::default()
        ),
        Err(RocksDbWireError::InvalidSequenceNumber { .. })
    ));

    let mut two = Vec::new();
    key_record(DELETION, b"one", &mut two);
    key_record(DELETION, b"two", &mut two);
    assert!(matches!(
        decode_write_batch(
            &batch(ROCKSDB_MAX_SEQUENCE_NUMBER, 2, two),
            WriteBatchLimits::default()
        ),
        Err(RocksDbWireError::InvalidSequenceNumber { .. })
    ));
}

#[test]
fn enforces_batch_mutation_auxiliary_key_and_value_limits() {
    let empty = batch(0, 0, Vec::new());
    let limits = WriteBatchLimits {
        max_batch_bytes: empty.len() - 1,
        ..WriteBatchLimits::default()
    };
    assert!(matches!(
        decode_write_batch(&empty, limits),
        Err(RocksDbWireError::WriteBatchLengthLimit { .. })
    ));

    let limits = WriteBatchLimits {
        max_mutations: 0,
        ..WriteBatchLimits::default()
    };
    assert!(matches!(
        decode_write_batch(&batch(0, 1, Vec::new()), limits),
        Err(RocksDbWireError::WriteBatchMutationLimit { count: 1, .. })
    ));

    let limits = WriteBatchLimits {
        max_auxiliary_records: 0,
        ..WriteBatchLimits::default()
    };
    assert!(matches!(
        decode_write_batch(&batch(0, 0, vec![NOOP]), limits),
        Err(RocksDbWireError::WriteBatchAuxiliaryRecordLimit { limit: 0 })
    ));

    let mut key = Vec::new();
    key_record(DELETION, b"12345", &mut key);
    let limits = WriteBatchLimits {
        max_key_bytes: 4,
        ..WriteBatchLimits::default()
    };
    assert!(matches!(
        decode_write_batch(&batch(0, 1, key), limits),
        Err(RocksDbWireError::FieldLengthLimit {
            context: "WriteBatch key",
            ..
        })
    ));

    let mut value = Vec::new();
    key_value_record(VALUE, b"k", b"12345", &mut value);
    let limits = WriteBatchLimits {
        max_value_bytes: 4,
        ..WriteBatchLimits::default()
    };
    assert!(matches!(
        decode_write_batch(&batch(0, 1, value), limits),
        Err(RocksDbWireError::FieldLengthLimit {
            context: "WriteBatch value",
            ..
        })
    ));
}

#[test]
fn rejects_noncanonical_truncated_and_unsupported_records() {
    let mut noncanonical = vec![DELETION, 0x80, 0x00];
    let bytes = batch(0, 1, std::mem::take(&mut noncanonical));
    assert!(matches!(
        decode_write_batch(&bytes, WriteBatchLimits::default()),
        Err(RocksDbWireError::NonCanonicalVarint {
            context: "WriteBatch key",
            ..
        })
    ));

    let mut valid = Vec::new();
    key_value_record(VALUE, b"key", b"value", &mut valid);
    let bytes = batch(0, 1, valid);
    for length in 0..bytes.len() {
        assert!(
            decode_write_batch(&bytes[..length], WriteBatchLimits::default()).is_err(),
            "truncated prefix length {length} unexpectedly decoded"
        );
    }

    for tag in [0x09, 0x10, 0x14, 0x16] {
        assert!(matches!(
            decode_write_batch(&batch(0, 0, vec![tag]), WriteBatchLimits::default()),
            Err(RocksDbWireError::UnsupportedWriteBatchTag { tag: actual, .. })
                if actual == tag
        ));
    }
    assert!(matches!(
        decode_write_batch(&batch(0, 0, vec![0xff]), WriteBatchLimits::default()),
        Err(RocksDbWireError::InvalidWriteBatchTag { tag: 0xff, .. })
    ));
}

fn batch(sequence: u64, count: u32, records: Vec<u8>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(WRITE_BATCH_HEADER_SIZE + records.len());
    bytes.extend_from_slice(&sequence.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&records);
    bytes
}

fn key_record(tag: u8, key: &[u8], output: &mut Vec<u8>) {
    output.push(tag);
    length_prefixed(key, output);
}

fn key_value_record(tag: u8, key: &[u8], value: &[u8], output: &mut Vec<u8>) {
    key_record(tag, key, output);
    length_prefixed(value, output);
}

fn range_record(tag: u8, start: &[u8], end: &[u8], output: &mut Vec<u8>) {
    key_value_record(tag, start, end, output);
}

fn cf_key_record(tag: u8, column_family: u32, key: &[u8], output: &mut Vec<u8>) {
    output.push(tag);
    varint(u64::from(column_family), output);
    length_prefixed(key, output);
}

fn cf_key_value_record(
    tag: u8,
    column_family: u32,
    key: &[u8],
    value: &[u8],
    output: &mut Vec<u8>,
) {
    cf_key_record(tag, column_family, key, output);
    length_prefixed(value, output);
}

fn cf_range_record(tag: u8, column_family: u32, start: &[u8], end: &[u8], output: &mut Vec<u8>) {
    cf_key_value_record(tag, column_family, start, end, output);
}

fn length_prefixed(value: &[u8], output: &mut Vec<u8>) {
    varint(value.len() as u64, output);
    output.extend_from_slice(value);
}

fn varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push(value as u8 | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}
