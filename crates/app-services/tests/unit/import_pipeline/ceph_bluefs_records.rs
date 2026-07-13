#[test]
fn aggregate_mapping_preserves_replay_paths_and_extents() {
    let inventory_id = "ceph-osd:test";
    let superblock = ceph_wire::BluefsSuper {
        uuid: uuid::Uuid::new_v4(),
        osd_uuid: uuid::Uuid::new_v4(),
        seq: 50,
        block_size: 4096,
        log_fnode: fnode(1, 0x100000),
        memorized_layout: Some(ceph_wire::BluefsLayout {
            shared_bdev: 1,
            dedicated_db: false,
            dedicated_wal: false,
            struct_version: 1,
            struct_compat_version: 1,
        }),
        crc32c: 0x1234_5678,
        struct_version: 2,
        struct_compat_version: 1,
    };
    let snapshot = crate::import_pipeline::ceph_bluefs_replay::BluefsReplaySnapshot {
        transaction_count: 4,
        first_sequence: 1,
        final_sequence: 100,
        logical_bytes: 0x22000,
        stop_reason: "invalidTail".to_string(),
        directories: vec!["db".to_string()],
        files: vec![
            crate::import_pipeline::ceph_bluefs_replay::BluefsReplayFile {
                path: "db/CURRENT".to_string(),
                inode: 2,
                fnode: fnode(2, 0x200000),
            },
        ],
    };

    let aggregate = super::build_bluefs_aggregate("source-1", inventory_id, superblock, snapshot);

    assert_eq!(aggregate.superblock.inventory_id, inventory_id);
    assert_eq!(aggregate.log_extents.len(), 1);
    assert_eq!(aggregate.replay.replay.transaction_count, 4);
    assert_eq!(aggregate.replay.directories[0].path, "db");
    assert_eq!(aggregate.replay.files[0].path, "db/CURRENT");
    assert_eq!(aggregate.replay.file_extents[0].file_path, "db/CURRENT");
    assert_eq!(aggregate.replay.file_extents[0].offset, 0x200000);
}

fn fnode(inode: u64, extent_offset: u64) -> ceph_wire::BluefsFnode {
    ceph_wire::BluefsFnode {
        ino: inode,
        size: 4096,
        mtime: ceph_wire::CephUtime {
            seconds: 1,
            nanoseconds: 2,
        },
        extents: vec![ceph_wire::BluefsExtent {
            offset: extent_offset,
            length: 4096,
            bdev: 1,
            struct_version: 1,
            struct_compat_version: 1,
        }],
        encoding: 0,
        content_size: 4096,
        struct_version: 2,
        struct_compat_version: 1,
    }
}
