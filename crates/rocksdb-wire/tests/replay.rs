use rocksdb_wire::{
    replay_version_edits, ColumnFamilyAction, InternalKeyMetadata, NewFile, NewFileFormat,
    NewFileMetadata, ReplayLimits, RocksDbWireError, VersionEdit,
};

fn model_internal_key(sequence: u64, value_type: u8) -> InternalKeyMetadata {
    InternalKeyMetadata {
        encoded_length: 9,
        user_key_length: 1,
        sequence_number: sequence,
        value_type,
    }
}

fn model_new_file(
    level: u32,
    file_number: u64,
    smallest_sequence_number: u64,
    largest_sequence_number: u64,
) -> NewFile {
    NewFile {
        format: NewFileFormat::NewFile4,
        level,
        file_number,
        path_id: 0,
        file_size: 4096,
        smallest: model_internal_key(largest_sequence_number, 1),
        largest: model_internal_key(smallest_sequence_number, 1),
        smallest_sequence_number,
        largest_sequence_number,
        metadata: NewFileMetadata::default(),
    }
}

fn recovery_edit(log: u64, next: u64, last: u64) -> VersionEdit {
    VersionEdit {
        comparator: Some(b"leveldb.BytewiseComparator".to_vec()),
        log_number: Some(log),
        previous_log_number: Some(0),
        next_file_number: Some(next),
        last_sequence: Some(last),
        ..VersionEdit::default()
    }
}

#[test]
fn replays_column_families_and_live_files_deterministically() {
    let mut base = recovery_edit(100, 1000, 500);
    base.max_column_family_id = Some(1);
    base.new_files.push(model_new_file(0, 10, 1, 100));

    let mut add = recovery_edit(101, 1000, 500);
    add.column_family_id = 1;
    add.column_family_action = Some(ColumnFamilyAction::Add {
        name: b"m-0".to_vec(),
    });

    let mut cf_file = VersionEdit {
        column_family_id: 1,
        log_number: Some(101),
        next_file_number: Some(1000),
        last_sequence: Some(500),
        ..VersionEdit::default()
    };
    cf_file.new_files.push(model_new_file(2, 20, 10, 200));

    let drop = VersionEdit {
        column_family_id: 1,
        column_family_action: Some(ColumnFamilyAction::Drop),
        next_file_number: Some(1000),
        last_sequence: Some(500),
        max_column_family_id: Some(1),
        ..VersionEdit::default()
    };

    let snapshot =
        replay_version_edits(&[base, add, cf_file, drop], ReplayLimits::default()).expect("replay");
    assert_eq!(snapshot.logical_edit_count, 4);
    assert_eq!(
        snapshot.comparator.as_deref(),
        Some(b"leveldb.BytewiseComparator".as_slice())
    );
    assert_eq!(snapshot.log_number, 101);
    assert_eq!(snapshot.next_file_number, 1001);
    assert_eq!(snapshot.last_sequence, 500);
    assert_eq!(snapshot.max_column_family_id, 1);
    assert_eq!(snapshot.column_families.len(), 2);
    assert_eq!(snapshot.column_families[0].id, 0);
    assert_eq!(snapshot.column_families[1].id, 1);
    assert!(snapshot.column_families[1].dropped);
    assert_eq!(snapshot.live_files.len(), 1);
    assert_eq!(snapshot.live_files[0].file_number, 10);
}

#[test]
fn rejects_duplicate_file_numbers_at_same_or_conflicting_locations() {
    let mut first = recovery_edit(10, 100, 50);
    first.new_files.push(model_new_file(0, 9, 1, 10));
    let mut duplicate = VersionEdit {
        log_number: Some(10),
        next_file_number: Some(100),
        last_sequence: Some(50),
        ..VersionEdit::default()
    };
    duplicate.new_files.push(model_new_file(0, 9, 2, 20));
    assert!(matches!(
        replay_version_edits(&[first.clone(), duplicate], ReplayLimits::default()),
        Err(RocksDbWireError::LiveFileConflict { file_number: 9, .. })
    ));

    let mut conflict = VersionEdit {
        log_number: Some(10),
        next_file_number: Some(100),
        last_sequence: Some(50),
        ..VersionEdit::default()
    };
    conflict.new_files.push(model_new_file(1, 9, 2, 20));
    assert!(matches!(
        replay_version_edits(&[first, conflict], ReplayLimits::default()),
        Err(RocksDbWireError::LiveFileConflict { file_number: 9, .. })
    ));
}

#[test]
fn validates_atomic_group_countdown_and_completion() {
    let first = VersionEdit {
        atomic_group_remaining: Some(1),
        log_number: Some(10),
        next_file_number: Some(100),
        last_sequence: Some(50),
        ..VersionEdit::default()
    };
    let second = VersionEdit {
        atomic_group_remaining: Some(0),
        log_number: Some(10),
        next_file_number: Some(100),
        last_sequence: Some(50),
        ..VersionEdit::default()
    };
    replay_version_edits(&[first.clone(), second], ReplayLimits::default())
        .expect("valid atomic group");

    assert!(matches!(
        replay_version_edits(std::slice::from_ref(&first), ReplayLimits::default()),
        Err(RocksDbWireError::InvalidAtomicGroup { .. })
    ));
    assert!(matches!(
        replay_version_edits(
            &[first, recovery_edit(10, 100, 50)],
            ReplayLimits::default()
        ),
        Err(RocksDbWireError::InvalidAtomicGroup { .. })
    ));
}

#[test]
fn rejects_missing_column_families_and_missing_deletions() {
    let missing_cf = VersionEdit {
        column_family_id: 7,
        log_number: Some(10),
        next_file_number: Some(100),
        last_sequence: Some(50),
        ..VersionEdit::default()
    };
    assert!(matches!(
        replay_version_edits(&[missing_cf], ReplayLimits::default()),
        Err(RocksDbWireError::MissingColumnFamily {
            column_family_id: 7,
            ..
        })
    ));

    let mut missing_file = recovery_edit(10, 100, 50);
    missing_file.deleted_files.push(rocksdb_wire::DeletedFile {
        level: 0,
        file_number: 9,
    });
    assert!(matches!(
        replay_version_edits(&[missing_file], ReplayLimits::default()),
        Err(RocksDbWireError::MissingLiveFile { file_number: 9, .. })
    ));
}

#[test]
fn enforces_monotonic_recovery_fields_and_required_fields() {
    let first = recovery_edit(10, 100, 50);
    let lower_last = recovery_edit(10, 100, 49);
    assert!(matches!(
        replay_version_edits(&[first.clone(), lower_last], ReplayLimits::default()),
        Err(RocksDbWireError::NonMonotonicField {
            field: "last sequence",
            ..
        })
    ));

    let lower_log = recovery_edit(9, 100, 50);
    assert!(matches!(
        replay_version_edits(&[first, lower_log], ReplayLimits::default()),
        Err(RocksDbWireError::NonMonotonicField {
            field: "column family log number",
            ..
        })
    ));

    let lower_next = [recovery_edit(10, 100, 50), recovery_edit(10, 99, 50)];
    assert!(matches!(
        replay_version_edits(&lower_next, ReplayLimits::default()),
        Err(RocksDbWireError::NonMonotonicField {
            field: "next file number",
            ..
        })
    ));

    assert_eq!(
        replay_version_edits(&[VersionEdit::default()], ReplayLimits::default()),
        Err(RocksDbWireError::MissingRecoveryField {
            field: "log number"
        })
    );
}

#[test]
fn enforces_live_file_and_column_family_limits() {
    let mut edit = recovery_edit(10, 100, 50);
    edit.new_files.push(model_new_file(0, 9, 1, 10));
    let limits = ReplayLimits {
        max_live_files: 0,
        ..ReplayLimits::default()
    };
    assert_eq!(
        replay_version_edits(&[edit], limits),
        Err(RocksDbWireError::LiveFileLimit { limit: 0 })
    );

    let mut add = recovery_edit(10, 100, 50);
    add.column_family_id = 1;
    add.column_family_action = Some(ColumnFamilyAction::Add {
        name: b"m-0".to_vec(),
    });
    let limits = ReplayLimits {
        max_column_families: 1,
        ..ReplayLimits::default()
    };
    assert_eq!(
        replay_version_edits(&[add], limits),
        Err(RocksDbWireError::ColumnFamilyLimit { limit: 1 })
    );
}
