use rocksdb_wire::{
    ColumnFamilyState, InternalKeyMetadata, LiveFile, ManifestSnapshot, NewFileFormat,
    NewFileMetadata,
};

#[test]
fn maps_manifest_column_families_and_live_files_without_key_bytes() {
    let control = control_files();
    let snapshot = snapshot();

    let records =
        super::build_rocksdb_aggregate("source-1", "inventory-1", control, snapshot).unwrap();

    assert_eq!(records.manifest.logical_edit_count, 39);
    assert_eq!(records.manifest.identity_uuid.as_deref(), Some(IDENTITY));
    assert_eq!(records.column_families.len(), 2);
    assert_eq!(records.column_families[1].comparator_name, COMPARATOR);
    assert_eq!(records.live_ssts.len(), 2);
    assert_eq!(records.live_ssts[0].format, "newFile");
    assert_eq!(records.live_ssts[0].smallest_sequence, None);
    assert_eq!(records.live_ssts[1].format, "newFile4");
    assert_eq!(records.live_ssts[1].smallest_sequence, Some(10));
    assert_eq!(records.live_ssts[1].smallest_internal_key_length, 17);
}

#[test]
fn rejects_column_families_without_a_comparator() {
    let mut snapshot = snapshot();
    snapshot.column_families[1].comparator = None;

    assert!(
        super::build_rocksdb_aggregate("source-1", "inventory-1", control_files(), snapshot,)
            .is_err()
    );
}

#[test]
fn rejects_non_utf8_persistent_metadata() {
    let mut snapshot = snapshot();
    snapshot.column_families[1].name = vec![0xff];

    let error =
        super::build_rocksdb_aggregate("source-1", "inventory-1", control_files(), snapshot)
            .unwrap_err();

    assert!(error
        .message
        .contains("column family name is not valid UTF-8"));
}

const COMPARATOR: &str = "leveldb.BytewiseComparator";
const IDENTITY: &str = "318c61d3-7d8b-497a-b02a-d3683123595d";

fn control_files() -> crate::import_pipeline::ceph_rocksdb_control_files::RocksdbControlFiles {
    crate::import_pipeline::ceph_rocksdb_control_files::RocksdbControlFiles {
        manifest_path: "db/MANIFEST-000143".to_string(),
        manifest_file_number: 143,
        manifest_file_size: 7280,
        identity_uuid: Some(IDENTITY.to_string()),
        manifest_bytes: Vec::new(),
    }
}

fn snapshot() -> ManifestSnapshot {
    ManifestSnapshot {
        logical_edit_count: 39,
        comparator: Some(COMPARATOR.as_bytes().to_vec()),
        log_number: 127,
        previous_log_number: 0,
        next_file_number: 148,
        last_sequence: 1_077_117,
        min_log_number_to_keep: 127,
        max_column_family_id: 1,
        column_families: vec![column_family(0, "default"), column_family(1, "m-0")],
        live_files: vec![
            live_file(0, 140, NewFileFormat::NewFile),
            live_file(1, 141, NewFileFormat::NewFile4),
        ],
    }
}

fn column_family(id: u32, name: &str) -> ColumnFamilyState {
    ColumnFamilyState {
        id,
        name: name.as_bytes().to_vec(),
        dropped: false,
        comparator: Some(COMPARATOR.as_bytes().to_vec()),
        log_number: Some(127),
        added_at_edit: (id > 0).then_some(1),
        dropped_at_edit: None,
        last_edit_ordinal: 38,
    }
}

fn live_file(column_family_id: u32, file_number: u64, format: NewFileFormat) -> LiveFile {
    LiveFile {
        column_family_id,
        level: 0,
        file_number,
        path_id: 0,
        file_size: 4096,
        smallest: internal_key(17, 10),
        largest: internal_key(29, 20),
        smallest_sequence_number: if format == NewFileFormat::NewFile {
            (1u64 << 56) - 1
        } else {
            10
        },
        largest_sequence_number: if format == NewFileFormat::NewFile {
            0
        } else {
            20
        },
        format,
        metadata: NewFileMetadata::default(),
        edit_ordinal: 38,
    }
}

fn internal_key(encoded_length: u32, sequence_number: u64) -> InternalKeyMetadata {
    InternalKeyMetadata {
        encoded_length,
        user_key_length: encoded_length - 8,
        sequence_number,
        value_type: 1,
    }
}
