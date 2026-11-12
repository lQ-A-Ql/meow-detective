use std::sync::atomic::AtomicBool;

use ceph_wire::{BluefsExtent, BluefsFnode, CephUtime};
use domain::DataSourceId;
use persistence_sqlite::repositories::ceph_rocksdb_repo::{
    CephRocksdbAggregate, CephRocksdbColumnFamilyRecord, CephRocksdbManifestRecord,
};
use rocksdb_wire::LogicalLogRecord;
use tempfile::TempDir;

use crate::import_pipeline::ceph_bluefs_replay::BluefsReplayFile;
use crate::import_pipeline::ceph_rocksdb_spool::RocksdbRecoverySpool;

use super::{
    inventory_wal_records, ColumnFamilyInventory, LocatedRocksdbWal, WalInventoryOutput,
    WalSequenceState,
};

const VALUE: u8 = 0x01;
const LOG_DATA: u8 = 0x03;
const CF_VALUE: u8 = 0x05;
const NOOP: u8 = 0x0d;

#[test]
fn inventories_records_without_persisting_raw_payloads() {
    let records = vec![
        logical_record(0, 0, batch(101, 1, value_record(VALUE, 0, b"a", b"1"))),
        logical_record(1, 64, batch(102, 0, Vec::new())),
        logical_record(
            2,
            96,
            batch(
                102,
                2,
                [
                    value_record(CF_VALUE, 7, b"b", b"2"),
                    value_record(VALUE, 0, b"c", b"3"),
                ]
                .concat(),
            ),
        ),
    ];
    let file = wal_file();
    let located = LocatedRocksdbWal {
        wal_number: 142,
        path: file.path.clone(),
        post_manifest: false,
        file: &file,
    };
    let rocksdb = rocksdb();
    let column_families = ColumnFamilyInventory::from_rocksdb(&rocksdb);
    let mut sequence_state = WalSequenceState::default();
    let mut output = Vec::new();
    let case = TempDir::new().expect("case root");
    let mut spool =
        RocksdbRecoverySpool::create(case.path(), &DataSourceId("source-1".to_string()))
            .expect("create spool");

    let summary = inventory_wal_records(
        &records,
        &rocksdb,
        &located,
        &column_families,
        &AtomicBool::new(false),
        &mut WalInventoryOutput {
            sequence_state: &mut sequence_state,
            records: &mut output,
            spool: &mut spool,
        },
    )
    .expect("inventory WAL records");

    assert_eq!(summary.logical_record_count, 3);
    assert_eq!(summary.empty_batch_count, 1);
    assert_eq!(summary.mutation_count, 3);
    assert_eq!(summary.first_sequence, Some(101));
    assert_eq!(summary.last_sequence, Some(103));
    assert_eq!(summary.first_record_offset, Some(0));
    assert_eq!(summary.last_record_offset, Some(96));
    assert_eq!(output.len(), 3);
    assert_eq!(output[0].first_mutation_sequence, Some(101));
    assert_eq!(output[1].first_mutation_sequence, None);
    assert_eq!(output[2].last_mutation_sequence, Some(103));
}

#[test]
fn accepts_sequence_gaps_and_dropped_column_families() {
    let file = wal_file();
    let located = LocatedRocksdbWal {
        wal_number: 142,
        path: file.path.clone(),
        post_manifest: false,
        file: &file,
    };
    let mut rocksdb = rocksdb();
    rocksdb
        .column_families
        .push(dropped_column_family(8, "dropped"));

    let gap = vec![
        logical_record(0, 0, batch(1, 1, value_record(VALUE, 0, b"a", b"1"))),
        logical_record(1, 32, batch(3, 1, value_record(VALUE, 0, b"b", b"2"))),
    ];
    assert!(inventory(&gap, &rocksdb, &located, false).is_ok());

    let dropped = vec![logical_record(
        0,
        0,
        batch(1, 1, value_record(CF_VALUE, 8, b"a", b"1")),
    )];
    assert!(inventory(&dropped, &rocksdb, &located, false).is_ok());
}

#[test]
fn rejects_sequence_overlap_unknown_column_families_and_cancellation() {
    let file = wal_file();
    let located = LocatedRocksdbWal {
        wal_number: 142,
        path: file.path.clone(),
        post_manifest: false,
        file: &file,
    };
    let rocksdb = rocksdb();

    let overlap = vec![
        logical_record(
            0,
            0,
            batch(
                1,
                2,
                [
                    value_record(VALUE, 0, b"a", b"1"),
                    value_record(VALUE, 0, b"b", b"2"),
                ]
                .concat(),
            ),
        ),
        logical_record(1, 64, batch(2, 1, value_record(VALUE, 0, b"c", b"3"))),
    ];
    assert!(inventory(&overlap, &rocksdb, &located, false).is_err());

    let unknown = vec![logical_record(
        0,
        0,
        batch(1, 1, value_record(CF_VALUE, 8, b"a", b"1")),
    )];
    assert!(inventory(&unknown, &rocksdb, &located, false).is_err());

    let valid = vec![logical_record(
        0,
        0,
        batch(1, 1, value_record(VALUE, 0, b"a", b"1")),
    )];
    assert!(inventory(&valid, &rocksdb, &located, true).is_err());
}

#[test]
fn accepts_log_data_but_rejects_noop_for_ceph_recovery() {
    let file = wal_file();
    let located = LocatedRocksdbWal {
        wal_number: 142,
        path: file.path.clone(),
        post_manifest: false,
        file: &file,
    };
    let rocksdb = rocksdb();
    let log_data = vec![logical_record(
        0,
        0,
        batch(1, 0, auxiliary_record(LOG_DATA, Some(b"opaque"))),
    )];
    let summary = inventory(&log_data, &rocksdb, &located, false).expect("accept LogData");
    assert_eq!(summary.auxiliary_record_count, 1);

    let noop = vec![logical_record(
        0,
        0,
        batch(1, 0, auxiliary_record(NOOP, None)),
    )];
    assert!(inventory(&noop, &rocksdb, &located, false).is_err());
}

fn inventory(
    records: &[LogicalLogRecord],
    rocksdb: &CephRocksdbAggregate,
    located: &LocatedRocksdbWal<'_>,
    cancelled: bool,
) -> Result<
    persistence_sqlite::repositories::ceph_rocksdb_wal_repo::CephRocksdbWalFileRecord,
    transport::CommandError,
> {
    let column_families = ColumnFamilyInventory::from_rocksdb(rocksdb);
    let case = TempDir::new().expect("case root");
    let mut spool =
        RocksdbRecoverySpool::create(case.path(), &DataSourceId("source-1".to_string()))
            .expect("create spool");
    let mut sequence_state = WalSequenceState::default();
    let mut output = Vec::new();
    inventory_wal_records(
        records,
        rocksdb,
        located,
        &column_families,
        &AtomicBool::new(cancelled),
        &mut WalInventoryOutput {
            sequence_state: &mut sequence_state,
            records: &mut output,
            spool: &mut spool,
        },
    )
}

fn logical_record(ordinal: u64, physical_offset: u64, data: Vec<u8>) -> LogicalLogRecord {
    LogicalLogRecord {
        ordinal,
        physical_offset,
        recyclable_log_number: None,
        fragment_count: 1,
        data,
    }
}

fn batch(sequence: u64, count: u32, records: Vec<u8>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(12 + records.len());
    bytes.extend_from_slice(&sequence.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&records);
    bytes
}

fn value_record(tag: u8, column_family: u32, key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut bytes = vec![tag];
    if tag == CF_VALUE {
        varint(u64::from(column_family), &mut bytes);
    }
    length_prefixed(key, &mut bytes);
    length_prefixed(value, &mut bytes);
    bytes
}

fn auxiliary_record(tag: u8, value: Option<&[u8]>) -> Vec<u8> {
    let mut bytes = vec![tag];
    if let Some(value) = value {
        length_prefixed(value, &mut bytes);
    }
    bytes
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

fn wal_file() -> BluefsReplayFile {
    BluefsReplayFile {
        path: "db.wal/000142.log".to_string(),
        inode: 142,
        fnode: BluefsFnode {
            ino: 142,
            size: 4096,
            mtime: CephUtime {
                seconds: 1,
                nanoseconds: 0,
            },
            extents: vec![BluefsExtent {
                bdev: 1,
                offset: 8192,
                length: 4096,
                struct_version: 1,
                struct_compat_version: 1,
            }],
            encoding: 0,
            content_size: 4096,
            struct_version: 2,
            struct_compat_version: 1,
        },
    }
}

fn rocksdb() -> CephRocksdbAggregate {
    CephRocksdbAggregate {
        manifest: CephRocksdbManifestRecord {
            inventory_id: "inventory-1".to_string(),
            data_source_id: "source-1".to_string(),
            active_manifest_path: "db/MANIFEST-000143".to_string(),
            identity_uuid: None,
            manifest_file_number: 143,
            manifest_file_size: 4096,
            logical_edit_count: 1,
            comparator_name: "leveldb.BytewiseComparator".to_string(),
            last_sequence: 100,
            next_file_number: 148,
            log_number: 127,
            prev_log_number: 0,
            max_column_family_id: 7,
            min_log_number_to_keep: Some(127),
        },
        column_families: vec![column_family(0, "default"), column_family(7, "O-0")],
        live_ssts: Vec::new(),
    }
}

fn column_family(id: u32, name: &str) -> CephRocksdbColumnFamilyRecord {
    CephRocksdbColumnFamilyRecord {
        inventory_id: "inventory-1".to_string(),
        column_family_id: id,
        name: name.to_string(),
        comparator_name: "leveldb.BytewiseComparator".to_string(),
        log_number: Some(127),
        dropped: false,
    }
}

fn dropped_column_family(id: u32, name: &str) -> CephRocksdbColumnFamilyRecord {
    let mut column_family = column_family(id, name);
    column_family.dropped = true;
    column_family
}
