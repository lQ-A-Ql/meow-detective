use ceph_wire::{BluefsExtent, BluefsFnode, CephUtime};
use persistence_sqlite::repositories::ceph_rocksdb_repo::{
    CephRocksdbAggregate, CephRocksdbColumnFamilyRecord, CephRocksdbLiveSstRecord,
    CephRocksdbManifestRecord,
};

use super::{locate_live_rocksdb_ssts, BluefsReplayFile, BluefsReplaySnapshot};

fn replay(path: &str, size: u64, encoding: u8) -> BluefsReplaySnapshot {
    BluefsReplaySnapshot {
        transaction_count: 1,
        first_sequence: 1,
        final_sequence: 1,
        logical_bytes: 4096,
        stop_reason: "invalidTail".to_string(),
        directories: vec!["db".to_string()],
        files: vec![BluefsReplayFile {
            path: path.to_string(),
            inode: 2,
            fnode: BluefsFnode {
                ino: 2,
                size,
                mtime: CephUtime {
                    seconds: 1,
                    nanoseconds: 0,
                },
                extents: vec![BluefsExtent {
                    bdev: 1,
                    offset: 8192,
                    length: u32::try_from(size).expect("test size"),
                    struct_version: 1,
                    struct_compat_version: 1,
                }],
                encoding,
                content_size: size,
                struct_version: 2,
                struct_compat_version: 1,
            },
        }],
    }
}

fn rocksdb(file_number: u64, file_size: u64, path_id: u32) -> CephRocksdbAggregate {
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
            log_number: 1,
            prev_log_number: 0,
            max_column_family_id: 0,
            min_log_number_to_keep: None,
        },
        column_families: vec![CephRocksdbColumnFamilyRecord {
            inventory_id: "inventory-1".to_string(),
            column_family_id: 0,
            name: "default".to_string(),
            comparator_name: "leveldb.BytewiseComparator".to_string(),
            log_number: Some(127),
            dropped: false,
        }],
        live_ssts: vec![CephRocksdbLiveSstRecord {
            inventory_id: "inventory-1".to_string(),
            column_family_id: 0,
            level: 1,
            file_number,
            path_id,
            format: "newFile4".to_string(),
            file_size,
            smallest_sequence: Some(1),
            largest_sequence: Some(100),
            smallest_internal_key_length: 9,
            largest_internal_key_length: 10,
        }],
    }
}

#[test]
fn locates_exact_six_digit_live_sst_and_column_family() {
    let replay = replay("db/000146.sst", 8192, 0);
    let rocksdb = rocksdb(146, 8192, 0);

    let located = locate_live_rocksdb_ssts(&replay, &rocksdb).expect("locate live SST");

    assert_eq!(located.len(), 1);
    assert_eq!(located[0].path, "db/000146.sst");
    assert_eq!(located[0].file.inode, 2);
    assert_eq!(located[0].column_family.name, "default");
}

#[test]
fn rejects_missing_size_mismatched_and_encoded_live_ssts() {
    let rocksdb = rocksdb(146, 8192, 0);
    assert!(locate_live_rocksdb_ssts(&replay("db/000145.sst", 8192, 0), &rocksdb).is_err());
    assert!(locate_live_rocksdb_ssts(&replay("db/000146.sst", 4096, 0), &rocksdb).is_err());
    assert!(locate_live_rocksdb_ssts(&replay("db/000146.sst", 8192, 1), &rocksdb).is_err());
}

#[test]
fn rejects_non_default_db_path_and_accepts_wider_file_numbers() {
    assert!(
        locate_live_rocksdb_ssts(&replay("db/000146.sst", 8192, 0), &rocksdb(146, 8192, 1))
            .is_err()
    );
    let replay = replay("db/1000000.sst", 8192, 0);
    let rocksdb = rocksdb(1_000_000, 8192, 0);
    let located =
        locate_live_rocksdb_ssts(&replay, &rocksdb).expect("locate seven-digit RocksDB SST");
    assert_eq!(located[0].path, "db/1000000.sst");
}
