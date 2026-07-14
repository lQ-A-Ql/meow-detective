use ceph_wire::{BluefsExtent, BluefsFnode, CephUtime};
use persistence_sqlite::repositories::ceph_rocksdb_repo::{
    CephRocksdbAggregate, CephRocksdbColumnFamilyRecord, CephRocksdbManifestRecord,
};

use super::{locate_active_rocksdb_wals, BluefsReplayFile, BluefsReplaySnapshot};

#[test]
fn selects_canonical_wals_at_or_above_recovery_lower_bound() {
    let replay = replay(
        &["db.wal"],
        &[
            wal_file("db.wal/000126.log", 126, 0),
            wal_file("db.wal/000127.log", 127, 0),
            wal_file("db.wal/000142.log", 142, 0),
            wal_file("db/000146.sst", 146, 0),
        ],
    );
    let rocksdb = rocksdb(Some(120), &[Some(127), Some(130)]);

    let selected = locate_active_rocksdb_wals(&replay, &rocksdb).expect("locate active WALs");

    assert_eq!(selected.recovery_lower_bound, 127);
    assert_eq!(
        selected
            .files
            .iter()
            .map(|file| file.wal_number)
            .collect::<Vec<_>>(),
        vec![127, 142]
    );
    assert!(!selected.files[0].post_manifest);
}

#[test]
fn min_log_number_to_keep_can_raise_the_recovery_boundary() {
    let replay = replay(
        &["db.wal"],
        &[
            wal_file("db.wal/000127.log", 127, 0),
            wal_file("db.wal/000142.log", 142, 0),
        ],
    );
    let rocksdb = rocksdb(Some(140), &[Some(127), Some(130)]);

    let selected = locate_active_rocksdb_wals(&replay, &rocksdb).expect("locate active WALs");

    assert_eq!(selected.recovery_lower_bound, 140);
    assert_eq!(
        selected
            .files
            .iter()
            .map(|file| file.wal_number)
            .collect::<Vec<_>>(),
        vec![142]
    );
}

#[test]
fn missing_log_numbers_resolve_to_zero_and_legacy_db_is_supported() {
    let replay = replay(
        &["db"],
        &[
            wal_file("db/000001.log", 1, 0),
            wal_file("db/000003.log", 3, 0),
        ],
    );
    let selected = locate_active_rocksdb_wals(&replay, &rocksdb(None, &[Some(2), None]))
        .expect("locate legacy WALs");

    assert_eq!(selected.recovery_lower_bound, 0);
    assert_eq!(
        selected
            .files
            .iter()
            .map(|file| file.wal_number)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
}

#[test]
fn rejects_noncanonical_and_duplicate_wal_paths() {
    let duplicate = replay(
        &["db.wal"],
        &[
            wal_file("db.wal/000142.log", 142, 0),
            wal_file("db.wal/000142.log", 143, 0),
        ],
    );
    assert!(locate_active_rocksdb_wals(&duplicate, &rocksdb(Some(127), &[Some(127)])).is_err());

    for path in [
        "db.wal/142.log",
        "db.wal/000000.log",
        "db.wal/not-a-number.log",
        "db.wal/nested/000142.log",
    ] {
        assert!(
            locate_active_rocksdb_wals(
                &replay(&["db.wal"], &[wal_file(path, 142, 0)]),
                &rocksdb(Some(127), &[Some(127)]),
            )
            .is_err(),
            "{path} unexpectedly passed"
        );
    }
}

#[test]
fn rejects_encoded_wals_but_accepts_empty_and_post_manifest_wals() {
    assert!(locate_active_rocksdb_wals(
        &replay(&["db.wal"], &[wal_file("db.wal/000142.log", 142, 1)]),
        &rocksdb(Some(127), &[Some(127)]),
    )
    .is_err());
    let replay = replay(
        &["db.wal"],
        &[
            empty_wal("db.wal/000142.log", 142),
            wal_file("db.wal/000148.log", 148, 0),
        ],
    );
    let selected = locate_active_rocksdb_wals(&replay, &rocksdb(Some(127), &[Some(127)]))
        .expect("locate empty active WAL");
    assert_eq!(selected.files.len(), 2);
    assert_eq!(selected.files[0].file.fnode.size, 0);
    assert!(!selected.files[0].post_manifest);
    assert!(selected.files[1].post_manifest);
}

#[test]
fn db_wal_takes_precedence_over_legacy_db() {
    let replay = replay(
        &["db", "db.wal"],
        &[
            wal_file("db/000127.log", 127, 0),
            wal_file("db.wal/000142.log", 142, 0),
        ],
    );
    let selected = locate_active_rocksdb_wals(&replay, &rocksdb(Some(127), &[Some(127)]))
        .expect("locate preferred WAL root");

    assert_eq!(selected.files.len(), 1);
    assert_eq!(selected.files[0].path, "db.wal/000142.log");
}

fn replay(directories: &[&str], files: &[BluefsReplayFile]) -> BluefsReplaySnapshot {
    BluefsReplaySnapshot {
        transaction_count: 1,
        first_sequence: 1,
        final_sequence: 1,
        logical_bytes: 4096,
        stop_reason: "invalidTail".to_string(),
        directories: directories
            .iter()
            .map(|directory| (*directory).to_string())
            .collect(),
        files: files.to_vec(),
    }
}

fn wal_file(path: &str, inode: u64, encoding: u8) -> BluefsReplayFile {
    BluefsReplayFile {
        path: path.to_string(),
        inode,
        fnode: BluefsFnode {
            ino: inode,
            size: 4096,
            mtime: CephUtime {
                seconds: 1,
                nanoseconds: 0,
            },
            extents: vec![BluefsExtent {
                bdev: 1,
                offset: 8192 + inode * 4096,
                length: 4096,
                struct_version: 1,
                struct_compat_version: 1,
            }],
            encoding,
            content_size: 4096,
            struct_version: 2,
            struct_compat_version: 1,
        },
    }
}

fn empty_wal(path: &str, inode: u64) -> BluefsReplayFile {
    let mut file = wal_file(path, inode, 0);
    file.fnode.size = 0;
    file
}

fn rocksdb(
    min_log_number_to_keep: Option<u64>,
    log_numbers: &[Option<u64>],
) -> CephRocksdbAggregate {
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
            max_column_family_id: log_numbers.len().saturating_sub(1) as u32,
            min_log_number_to_keep,
        },
        column_families: log_numbers
            .iter()
            .enumerate()
            .map(|(index, log_number)| CephRocksdbColumnFamilyRecord {
                inventory_id: "inventory-1".to_string(),
                column_family_id: index as u32,
                name: format!("cf-{index}"),
                comparator_name: "leveldb.BytewiseComparator".to_string(),
                log_number: *log_number,
                dropped: false,
            })
            .collect(),
        live_ssts: Vec::new(),
    }
}
