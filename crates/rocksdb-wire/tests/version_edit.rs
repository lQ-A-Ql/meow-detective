use rocksdb_wire::{
    parse_version_edit, ColumnFamilyAction, NewFileFormat, RocksDbWireError, VersionEditLimits,
};

const MAX_SEQUENCE: u64 = (1u64 << 56) - 1;

fn put_varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push((value as u8) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn put_length_prefixed(value: &[u8], output: &mut Vec<u8>) {
    put_varint(value.len() as u64, output);
    output.extend_from_slice(value);
}

fn put_tag_varint(tag: u32, value: u64, output: &mut Vec<u8>) {
    put_varint(u64::from(tag), output);
    put_varint(value, output);
}

fn put_tag_bytes(tag: u32, value: &[u8], output: &mut Vec<u8>) {
    put_varint(u64::from(tag), output);
    put_length_prefixed(value, output);
}

fn internal_key(user_key: &[u8], sequence: u64, value_type: u8) -> Vec<u8> {
    let mut key = user_key.to_vec();
    key.extend_from_slice(&((sequence << 8) | u64::from(value_type)).to_le_bytes());
    key
}

fn put_new_file_core(
    output: &mut Vec<u8>,
    level: u32,
    file_number: u64,
    file_size: u64,
    smallest: &[u8],
    largest: &[u8],
) {
    put_varint(u64::from(level), output);
    put_varint(file_number, output);
    put_varint(file_size, output);
    put_length_prefixed(smallest, output);
    put_length_prefixed(largest, output);
}

#[test]
fn parses_stage4_fields_and_all_new_file_formats() {
    let smallest = internal_key(b"a", 40, 1);
    let largest = internal_key(b"z", 10, 0);
    let mut encoded = Vec::new();
    put_tag_bytes(1, b"leveldb.BytewiseComparator", &mut encoded);
    put_tag_varint(2, 142, &mut encoded);
    put_tag_varint(9, 0, &mut encoded);
    put_tag_varint(3, 148, &mut encoded);
    put_tag_varint(4, 1_077_117, &mut encoded);
    put_tag_varint(10, 127, &mut encoded);
    put_tag_varint(203, 11, &mut encoded);

    put_varint(5, &mut encoded);
    put_varint(1, &mut encoded);
    put_length_prefixed(&smallest, &mut encoded);
    put_varint(6, &mut encoded);
    put_varint(2, &mut encoded);
    put_varint(17, &mut encoded);

    put_varint(7, &mut encoded);
    put_new_file_core(&mut encoded, 0, 20, 100, &smallest, &largest);

    put_varint(100, &mut encoded);
    put_new_file_core(&mut encoded, 1, 21, 200, &smallest, &largest);
    put_varint(10, &mut encoded);
    put_varint(40, &mut encoded);

    put_varint(102, &mut encoded);
    put_varint(2, &mut encoded);
    put_varint(22, &mut encoded);
    put_varint(2, &mut encoded);
    put_varint(300, &mut encoded);
    put_length_prefixed(&smallest, &mut encoded);
    put_length_prefixed(&largest, &mut encoded);
    put_varint(11, &mut encoded);
    put_varint(41, &mut encoded);

    put_varint(103, &mut encoded);
    put_new_file_core(&mut encoded, 3, 23, 400, &smallest, &largest);
    put_varint(12, &mut encoded);
    put_varint(42, &mut encoded);
    put_tag_bytes(2, &[1], &mut encoded);
    put_tag_bytes(3, &127u64.to_le_bytes(), &mut encoded);
    put_tag_bytes(5, &[9], &mut encoded);
    put_tag_bytes(6, &[10], &mut encoded);
    put_tag_bytes(7, b"abcd", &mut encoded);
    put_tag_bytes(8, b"crc32c", &mut encoded);
    put_tag_bytes(9, &[4], &mut encoded);
    put_tag_bytes(10, b"safe timestamp", &mut encoded);
    put_tag_bytes(12, &[0x55; 16], &mut encoded);
    put_tag_bytes(13, &[11], &mut encoded);
    put_tag_bytes(14, &[12], &mut encoded);
    put_tag_bytes(65, &[3], &mut encoded);
    put_tag_bytes(33, b"future", &mut encoded);
    put_varint(1, &mut encoded);

    put_tag_bytes(0x2710, b"ignored top-level", &mut encoded);

    let edit = parse_version_edit(&encoded, VersionEditLimits::default()).expect("parse edit");
    assert_eq!(
        edit.comparator.as_deref(),
        Some(b"leveldb.BytewiseComparator".as_slice())
    );
    assert_eq!(edit.log_number, Some(142));
    assert_eq!(edit.previous_log_number, Some(0));
    assert_eq!(edit.next_file_number, Some(148));
    assert_eq!(edit.last_sequence, Some(1_077_117));
    assert_eq!(edit.min_log_number_to_keep, Some(127));
    assert_eq!(edit.max_column_family_id, Some(11));
    assert_eq!(edit.compact_cursors.len(), 1);
    assert_eq!(edit.deleted_files.len(), 1);
    assert_eq!(edit.new_files.len(), 4);
    assert_eq!(edit.new_files[0].format, NewFileFormat::NewFile);
    assert_eq!(edit.new_files[0].smallest_sequence_number, MAX_SEQUENCE);
    assert_eq!(edit.new_files[1].format, NewFileFormat::NewFile2);
    assert_eq!(edit.new_files[2].path_id, 2);
    assert_eq!(edit.new_files[3].format, NewFileFormat::NewFile4);
    assert_eq!(edit.new_files[3].path_id, 3);
    assert!(edit.new_files[3].metadata.marked_for_compaction);
    assert_eq!(edit.new_files[3].metadata.epoch_number, Some(11));
    assert_eq!(edit.new_files[3].metadata.skipped_safe_custom_fields, 2);
    assert_eq!(edit.ignored_fields[0].tag, 0x2710);
}

#[test]
fn parses_column_family_add_drop_and_atomic_group() {
    let mut add = Vec::new();
    put_tag_varint(200, 7, &mut add);
    put_tag_bytes(201, b"O-0", &mut add);
    put_tag_varint(203, 7, &mut add);
    put_tag_varint(300, 1, &mut add);
    let edit = parse_version_edit(&add, VersionEditLimits::default()).expect("parse add");
    assert_eq!(edit.column_family_id, 7);
    assert_eq!(
        edit.column_family_action,
        Some(ColumnFamilyAction::Add {
            name: b"O-0".to_vec()
        })
    );
    assert_eq!(edit.atomic_group_remaining, Some(1));

    let mut drop = Vec::new();
    put_tag_varint(200, 7, &mut drop);
    put_varint(202, &mut drop);
    assert_eq!(
        parse_version_edit(&drop, VersionEditLimits::default())
            .expect("parse drop")
            .column_family_action,
        Some(ColumnFamilyAction::Drop)
    );
}

#[test]
fn accepts_reef_min_log_pseudo_record_without_exposing_a_file() {
    let pseudo_key = internal_key(b"dummy_key", 0, 1);
    let mut encoded = Vec::new();
    put_varint(103, &mut encoded);
    put_new_file_core(&mut encoded, 0, 0, 0, &pseudo_key, &pseudo_key);
    put_varint(MAX_SEQUENCE, &mut encoded);
    put_varint(0, &mut encoded);
    put_tag_bytes(3, &127u64.to_le_bytes(), &mut encoded);
    put_varint(1, &mut encoded);

    let edit = parse_version_edit(&encoded, VersionEditLimits::default()).expect("parse pseudo");
    assert!(edit.new_files.is_empty());
    assert_eq!(edit.min_log_number_to_keep, Some(127));
}

#[test]
fn rejects_unknown_mandatory_tags_and_accepts_safe_unknowns() {
    let mut safe = Vec::new();
    put_tag_bytes(0x2710, b"abc", &mut safe);
    assert_eq!(
        parse_version_edit(&safe, VersionEditLimits::default())
            .expect("safe tag")
            .ignored_fields
            .len(),
        1
    );

    let mut mandatory = Vec::new();
    put_varint(666, &mut mandatory);
    assert_eq!(
        parse_version_edit(&mandatory, VersionEditLimits::default()),
        Err(RocksDbWireError::UnknownMandatoryTag { tag: 666 })
    );
}

#[test]
fn fails_closed_on_tracked_wal_manifest_edits() {
    for tag in [8196, 8197, 8199, 8200] {
        let mut encoded = Vec::new();
        put_varint(tag, &mut encoded);
        assert_eq!(
            parse_version_edit(&encoded, VersionEditLimits::default()),
            Err(RocksDbWireError::UnsupportedTrackedWalEdit { tag: tag as u32 })
        );
    }
}

#[test]
fn preserves_non_utf8_comparator_and_column_family_names() {
    let mut encoded = Vec::new();
    put_tag_bytes(1, &[0xff, 0x00], &mut encoded);
    put_tag_varint(200, 7, &mut encoded);
    put_tag_bytes(201, &[0xfe, 0x80], &mut encoded);

    let edit = parse_version_edit(&encoded, VersionEditLimits::default()).expect("parse bytes");
    assert_eq!(edit.comparator, Some(vec![0xff, 0x00]));
    assert_eq!(
        edit.column_family_action,
        Some(ColumnFamilyAction::Add {
            name: vec![0xfe, 0x80]
        })
    );
}

#[test]
fn enforces_new_file4_custom_skip_and_mandatory_rules() {
    let smallest = internal_key(b"a", 5, 1);
    let largest = internal_key(b"z", 1, 1);
    let mut safe = Vec::new();
    put_varint(103, &mut safe);
    put_new_file_core(&mut safe, 0, 9, 100, &smallest, &largest);
    put_varint(1, &mut safe);
    put_varint(5, &mut safe);
    put_tag_bytes(33, b"safe", &mut safe);
    put_varint(1, &mut safe);
    assert_eq!(
        parse_version_edit(&safe, VersionEditLimits::default())
            .expect("safe custom")
            .new_files[0]
            .metadata
            .skipped_safe_custom_fields,
        1
    );

    let mut mandatory = Vec::new();
    put_varint(103, &mut mandatory);
    put_new_file_core(&mut mandatory, 0, 9, 100, &smallest, &largest);
    put_varint(1, &mut mandatory);
    put_varint(5, &mut mandatory);
    put_tag_bytes(66, b"mandatory", &mut mandatory);
    put_varint(1, &mut mandatory);
    assert_eq!(
        parse_version_edit(&mandatory, VersionEditLimits::default()),
        Err(RocksDbWireError::UnknownMandatoryCustomTag { tag: 66 })
    );
}

#[test]
fn rejects_malformed_varints_duplicates_and_truncated_custom_fields() {
    assert!(matches!(
        parse_version_edit(&[0x80], VersionEditLimits::default()),
        Err(RocksDbWireError::UnexpectedEof { .. })
    ));
    assert!(matches!(
        parse_version_edit(&[0x81, 0x00], VersionEditLimits::default()),
        Err(RocksDbWireError::NonCanonicalVarint { .. })
    ));

    let mut duplicate = Vec::new();
    put_tag_bytes(1, b"a", &mut duplicate);
    put_tag_bytes(1, b"a", &mut duplicate);
    assert_eq!(
        parse_version_edit(&duplicate, VersionEditLimits::default()),
        Err(RocksDbWireError::DuplicateVersionEditField {
            field: "comparator"
        })
    );

    let key = internal_key(b"k", 1, 1);
    let mut missing_terminator = Vec::new();
    put_varint(103, &mut missing_terminator);
    put_new_file_core(&mut missing_terminator, 0, 9, 10, &key, &key);
    put_varint(1, &mut missing_terminator);
    put_varint(1, &mut missing_terminator);
    put_tag_bytes(33, b"safe", &mut missing_terminator);
    assert!(matches!(
        parse_version_edit(&missing_terminator, VersionEditLimits::default()),
        Err(RocksDbWireError::UnexpectedEof { .. })
    ));
}

#[test]
fn enforces_key_path_sequence_and_configured_limits() {
    let mut short_key = Vec::new();
    put_varint(7, &mut short_key);
    put_new_file_core(&mut short_key, 0, 9, 10, b"short", b"short");
    assert!(matches!(
        parse_version_edit(&short_key, VersionEditLimits::default()),
        Err(RocksDbWireError::InternalKeyTooShort { .. })
    ));

    let key = internal_key(b"k", 1, 1);
    let mut bad_path = Vec::new();
    put_varint(102, &mut bad_path);
    put_varint(0, &mut bad_path);
    put_varint(9, &mut bad_path);
    put_varint(4, &mut bad_path);
    put_varint(10, &mut bad_path);
    put_length_prefixed(&key, &mut bad_path);
    put_length_prefixed(&key, &mut bad_path);
    put_varint(1, &mut bad_path);
    put_varint(1, &mut bad_path);
    assert_eq!(
        parse_version_edit(&bad_path, VersionEditLimits::default()),
        Err(RocksDbWireError::InvalidPathId { path_id: 4 })
    );

    let limits = VersionEditLimits {
        max_tags: 0,
        ..VersionEditLimits::default()
    };
    assert_eq!(
        parse_version_edit(&[1], limits),
        Err(RocksDbWireError::VersionEditTagLimit { limit: 0 })
    );
}

#[test]
fn rejects_invalid_internal_key_types_and_file_sequence_ranges() {
    let invalid_type = internal_key(b"k", 1, 3);
    let mut bad_key = Vec::new();
    put_varint(7, &mut bad_key);
    put_new_file_core(&mut bad_key, 0, 9, 10, &invalid_type, &invalid_type);
    assert!(matches!(
        parse_version_edit(&bad_key, VersionEditLimits::default()),
        Err(RocksDbWireError::InvalidInternalKeyType { value_type: 3, .. })
    ));

    let key = internal_key(b"k", 1, 1);
    let mut overflow = Vec::new();
    put_varint(100, &mut overflow);
    put_new_file_core(&mut overflow, 0, 9, 10, &key, &key);
    put_varint(MAX_SEQUENCE + 1, &mut overflow);
    put_varint(MAX_SEQUENCE + 1, &mut overflow);
    assert_eq!(
        parse_version_edit(&overflow, VersionEditLimits::default()),
        Err(RocksDbWireError::InvalidSequenceNumber {
            sequence: MAX_SEQUENCE + 1
        })
    );

    let mut reversed = Vec::new();
    put_varint(100, &mut reversed);
    put_new_file_core(&mut reversed, 0, 9, 10, &key, &key);
    put_varint(8, &mut reversed);
    put_varint(7, &mut reversed);
    assert_eq!(
        parse_version_edit(&reversed, VersionEditLimits::default()),
        Err(RocksDbWireError::InvalidSequenceRange {
            smallest: 8,
            largest: 7
        })
    );
}

#[test]
fn enforces_custom_field_and_file_mutation_limits() {
    let key = internal_key(b"k", 1, 1);
    let mut custom = Vec::new();
    put_varint(103, &mut custom);
    put_new_file_core(&mut custom, 0, 9, 10, &key, &key);
    put_varint(1, &mut custom);
    put_varint(1, &mut custom);
    put_tag_bytes(33, b"safe", &mut custom);
    put_varint(1, &mut custom);
    let limits = VersionEditLimits {
        max_custom_fields_per_file: 0,
        ..VersionEditLimits::default()
    };
    assert_eq!(
        parse_version_edit(&custom, limits),
        Err(RocksDbWireError::CustomFieldCountLimit { limit: 0 })
    );

    let mut deleted = Vec::new();
    put_varint(6, &mut deleted);
    put_varint(0, &mut deleted);
    put_varint(9, &mut deleted);
    let limits = VersionEditLimits {
        max_file_mutations: 0,
        ..VersionEditLimits::default()
    };
    assert_eq!(
        parse_version_edit(&deleted, limits),
        Err(RocksDbWireError::FileMutationLimit { limit: 0 })
    );
}

#[test]
fn rejects_conflicting_direct_and_custom_min_log_numbers() {
    let key = internal_key(b"k", 1, 1);
    let mut encoded = Vec::new();
    put_tag_varint(10, 127, &mut encoded);
    put_varint(103, &mut encoded);
    put_new_file_core(&mut encoded, 0, 9, 10, &key, &key);
    put_varint(1, &mut encoded);
    put_varint(1, &mut encoded);
    put_tag_bytes(3, &128u64.to_le_bytes(), &mut encoded);
    put_varint(1, &mut encoded);

    assert_eq!(
        parse_version_edit(&encoded, VersionEditLimits::default()),
        Err(RocksDbWireError::ConflictingField {
            field: "minimum log number to keep",
            first: 127,
            second: 128
        })
    );
}
