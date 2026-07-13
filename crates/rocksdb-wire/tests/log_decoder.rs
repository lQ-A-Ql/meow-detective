use rocksdb_wire::{
    crc32c, decode_log, decode_manifest, extend_crc32c, mask_crc32c, LogDecodeLimits,
    LogDecodeOptions, ManifestDecodeLimits, RocksDbWireError, VersionEditLimits,
    ROCKSDB_LOG_BLOCK_SIZE,
};

const FULL: u8 = 1;
const FIRST: u8 = 2;
const MIDDLE: u8 = 3;
const LAST: u8 = 4;
const RECYCLABLE_FULL: u8 = 5;
const RECYCLABLE_FIRST: u8 = 6;
const RECYCLABLE_MIDDLE: u8 = 7;
const RECYCLABLE_LAST: u8 = 8;
const SET_COMPRESSION_TYPE: u8 = 9;

fn physical_record(record_type: u8, payload: &[u8], log_number: Option<u32>) -> Vec<u8> {
    let header_size = if log_number.is_some() { 11 } else { 7 };
    let mut record = vec![0u8; header_size];
    record[4..6].copy_from_slice(&(payload.len() as u16).to_le_bytes());
    record[6] = record_type;
    if let Some(log_number) = log_number {
        record[7..11].copy_from_slice(&log_number.to_le_bytes());
    }
    let header_crc = extend_crc32c(0, &record[6..header_size]);
    let crc = extend_crc32c(header_crc, payload);
    record[0..4].copy_from_slice(&mask_crc32c(crc).to_le_bytes());
    record.extend_from_slice(payload);
    record
}

#[test]
fn matches_official_crc32c_vector_and_mask_round_trip() {
    let crc = crc32c(&[0; 32]);
    assert_eq!(crc, 0x8a91_36aa);
    assert_eq!(rocksdb_wire::unmask_crc32c(mask_crc32c(crc)), crc);
}

#[test]
fn decodes_a_fixed_manifest_record_end_to_end() {
    let manifest = [
        0x50, 0x03, 0x0b, 0x79, 0x0b, 0x00, 0x01, 0x01, 0x03, 0x63, 0x6d, 0x70, 0x02, 0x07, 0x03,
        0x0a, 0x04, 0x05,
    ];
    let records = decode_log(&manifest, LogDecodeOptions::default()).expect("decode log");
    assert_eq!(
        records[0].data,
        [0x01, 0x03, 0x63, 0x6d, 0x70, 0x02, 0x07, 0x03, 0x0a, 0x04, 0x05]
    );
    let edit = rocksdb_wire::parse_version_edit(&records[0].data, VersionEditLimits::default())
        .expect("decode edit");
    assert_eq!(edit.next_file_number, Some(10));

    let snapshot =
        decode_manifest(&manifest, ManifestDecodeLimits::default()).expect("decode manifest");
    assert_eq!(snapshot.comparator.as_deref(), Some(b"cmp".as_slice()));
    assert_eq!(snapshot.log_number, 7);
    assert_eq!(snapshot.next_file_number, 11);
    assert_eq!(snapshot.last_sequence, 5);
}

#[test]
fn decodes_full_and_fragmented_records_across_blocks() {
    let mut input = physical_record(FULL, b"alpha", None);
    input.resize(ROCKSDB_LOG_BLOCK_SIZE, 0);

    let first_payload = vec![b'a'; ROCKSDB_LOG_BLOCK_SIZE - 7];
    input.extend_from_slice(&physical_record(FIRST, &first_payload, None));
    input.extend_from_slice(&physical_record(MIDDLE, b"middle", None));
    input.extend_from_slice(&physical_record(LAST, b"last", None));

    let records = decode_log(&input, LogDecodeOptions::default()).expect("decode log");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].data, b"alpha");
    assert_eq!(records[0].physical_offset, 0);
    assert_eq!(records[1].fragment_count, 3);
    assert_eq!(records[1].physical_offset, ROCKSDB_LOG_BLOCK_SIZE as u64);
    assert_eq!(records[1].data.len(), first_payload.len() + 10);
    assert!(records[1].data.ends_with(b"middlelast"));
}

#[test]
fn decodes_recyclable_full_and_fragmented_records() {
    let log_number = 143;
    let mut input = physical_record(RECYCLABLE_FULL, b"one", Some(log_number));
    input.extend_from_slice(&physical_record(
        RECYCLABLE_FIRST,
        b"two-",
        Some(log_number),
    ));
    input.extend_from_slice(&physical_record(
        RECYCLABLE_MIDDLE,
        b"three-",
        Some(log_number),
    ));
    input.extend_from_slice(&physical_record(RECYCLABLE_LAST, b"four", Some(log_number)));

    let records = decode_log(
        &input,
        LogDecodeOptions {
            expected_recyclable_log_number: Some(log_number),
            ..LogDecodeOptions::default()
        },
    )
    .expect("decode recyclable log");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].data, b"one");
    assert_eq!(records[1].data, b"two-three-four");
    assert_eq!(records[1].recyclable_log_number, Some(log_number));
}

#[test]
fn rejects_crc_corruption_and_unmasked_crc_storage() {
    let mut corrupted = physical_record(FULL, b"payload", None);
    corrupted[7] ^= 0x40;
    assert!(matches!(
        decode_log(&corrupted, LogDecodeOptions::default()),
        Err(RocksDbWireError::LogCrcMismatch { .. })
    ));

    let mut unmasked = physical_record(FULL, b"payload", None);
    let raw_crc = crc32c(&unmasked[6..]);
    unmasked[0..4].copy_from_slice(&raw_crc.to_le_bytes());
    assert!(matches!(
        decode_log(&unmasked, LogDecodeOptions::default()),
        Err(RocksDbWireError::LogCrcMismatch { .. })
    ));
}

#[test]
fn rejects_invalid_fragment_sequences() {
    let middle = physical_record(MIDDLE, b"orphan", None);
    assert!(matches!(
        decode_log(&middle, LogDecodeOptions::default()),
        Err(RocksDbWireError::InvalidFragmentSequence {
            actual: "MIDDLE",
            ..
        })
    ));

    let mut nested = physical_record(FIRST, b"first", None);
    nested.extend_from_slice(&physical_record(FIRST, b"nested", None));
    assert!(matches!(
        decode_log(&nested, LogDecodeOptions::default()),
        Err(RocksDbWireError::InvalidFragmentSequence {
            actual: "FIRST",
            ..
        })
    ));

    let missing_last = physical_record(FIRST, b"partial", None);
    assert!(matches!(
        decode_log(&missing_last, LogDecodeOptions::default()),
        Err(RocksDbWireError::UnterminatedLogicalRecord { .. })
    ));
}

#[test]
fn rejects_mixed_fragment_encoding_and_wrong_recyclable_number() {
    let mut mixed = physical_record(FIRST, b"first", None);
    mixed.extend_from_slice(&physical_record(RECYCLABLE_LAST, b"last", Some(9)));
    assert!(matches!(
        decode_log(
            &mixed,
            LogDecodeOptions {
                expected_recyclable_log_number: Some(9),
                ..LogDecodeOptions::default()
            }
        ),
        Err(RocksDbWireError::MixedFragmentEncoding { .. })
    ));

    let recyclable = physical_record(RECYCLABLE_FULL, b"old", Some(8));
    assert!(matches!(
        decode_log(
            &recyclable,
            LogDecodeOptions {
                expected_recyclable_log_number: Some(9),
                ..LogDecodeOptions::default()
            }
        ),
        Err(RocksDbWireError::RecyclableLogNumberMismatch {
            expected: 9,
            actual: 8,
            ..
        })
    ));

    assert!(matches!(
        decode_log(&recyclable, LogDecodeOptions::default()),
        Err(RocksDbWireError::RecyclableLogNumberRequired { .. })
    ));
}

#[test]
fn enforces_file_record_and_count_limits_before_growth() {
    let input = physical_record(FULL, b"12345", None);
    let limits = LogDecodeLimits {
        max_file_bytes: input.len() - 1,
        ..LogDecodeLimits::default()
    };
    assert!(matches!(
        decode_log(
            &input,
            LogDecodeOptions {
                limits,
                ..LogDecodeOptions::default()
            }
        ),
        Err(RocksDbWireError::LogLengthLimit { .. })
    ));

    let limits = LogDecodeLimits {
        max_logical_record_bytes: 4,
        ..LogDecodeLimits::default()
    };
    assert!(matches!(
        decode_log(
            &input,
            LogDecodeOptions {
                limits,
                ..LogDecodeOptions::default()
            }
        ),
        Err(RocksDbWireError::LogicalRecordLengthLimit { .. })
    ));

    let limits = LogDecodeLimits {
        max_logical_records: 0,
        ..LogDecodeLimits::default()
    };
    assert!(matches!(
        decode_log(
            &input,
            LogDecodeOptions {
                limits,
                ..LogDecodeOptions::default()
            }
        ),
        Err(RocksDbWireError::LogicalRecordCountLimit { limit: 0 })
    ));
}

#[test]
fn rejects_cross_block_lengths_and_nonzero_truncated_tails() {
    let first_payload = vec![b'x'; ROCKSDB_LOG_BLOCK_SIZE - 17];
    let mut input = physical_record(FULL, &first_payload, None);
    input.extend_from_slice(&[0, 0, 0, 0, 4, 0, FULL, 1, 2, 3]);
    assert_eq!(input.len(), ROCKSDB_LOG_BLOCK_SIZE);
    assert!(matches!(
        decode_log(&input, LogDecodeOptions::default()),
        Err(RocksDbWireError::CrossBlockRecord { .. })
    ));

    let mut tail = physical_record(FULL, b"ok", None);
    tail.extend_from_slice(&[1, 2, 3]);
    assert!(matches!(
        decode_log(&tail, LogDecodeOptions::default()),
        Err(RocksDbWireError::NonZeroLogTrailer { .. })
    ));
}

#[test]
fn rejects_truncated_final_bodies_and_wal_compression_controls() {
    let mut truncated = physical_record(FULL, b"body", None);
    truncated.pop();
    assert!(matches!(
        decode_log(&truncated, LogDecodeOptions::default()),
        Err(RocksDbWireError::TruncatedLogBody {
            declared: 4,
            available: 3,
            ..
        })
    ));

    let compression = physical_record(SET_COMPRESSION_TYPE, &[0], None);
    assert!(matches!(
        decode_log(&compression, LogDecodeOptions::default()),
        Err(RocksDbWireError::UnsupportedWalCompressionRecord { offset: 0 })
    ));
}
