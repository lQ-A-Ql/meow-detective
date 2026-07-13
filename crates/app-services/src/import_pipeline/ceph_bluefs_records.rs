use persistence_sqlite::repositories::{
    ceph_bluefs_replay_repo::{
        CephBluefsDirectoryRecord, CephBluefsFileExtentRecord, CephBluefsFileRecord,
        CephBluefsReplayAggregate, CephBluefsReplayRecord,
    },
    ceph_bluefs_repo::{
        CephBluefsAggregate, CephBluefsLogExtentRecord, CephBluefsSuperblockRecord,
    },
};

use super::ceph_bluefs_replay::BluefsReplaySnapshot;

pub(super) fn build_bluefs_aggregate(
    data_source_id: &str,
    inventory_id: &str,
    superblock: ceph_wire::BluefsSuper,
    replay: BluefsReplaySnapshot,
) -> CephBluefsAggregate {
    let log_extents = superblock
        .log_fnode
        .extents
        .iter()
        .enumerate()
        .map(|(ordinal, extent)| CephBluefsLogExtentRecord {
            inventory_id: inventory_id.to_string(),
            ordinal: ordinal as u32,
            device_id: extent.bdev,
            offset: extent.offset,
            length: extent.length,
        })
        .collect();
    let replay_records = build_replay_records(inventory_id, replay);
    let layout = superblock.memorized_layout.as_ref();
    let superblock_record = CephBluefsSuperblockRecord {
        inventory_id: inventory_id.to_string(),
        data_source_id: data_source_id.to_string(),
        bluefs_uuid: superblock.uuid.to_string(),
        osd_uuid: superblock.osd_uuid.to_string(),
        sequence: superblock.seq,
        block_size: superblock.block_size,
        crc32c: superblock.crc32c,
        struct_version: superblock.struct_version,
        struct_compat_version: superblock.struct_compat_version,
        log_inode: superblock.log_fnode.ino,
        log_size: superblock.log_fnode.size,
        log_mtime_seconds: i64::from(superblock.log_fnode.mtime.seconds),
        log_mtime_nanoseconds: superblock.log_fnode.mtime.nanoseconds,
        log_encoding: superblock.log_fnode.encoding,
        log_content_size: superblock.log_fnode.content_size,
        shared_bdev: layout.map(|value| value.shared_bdev),
        dedicated_db: layout.map(|value| value.dedicated_db),
        dedicated_wal: layout.map(|value| value.dedicated_wal),
    };
    CephBluefsAggregate {
        superblock: superblock_record,
        log_extents,
        replay: replay_records,
    }
}

fn build_replay_records(
    inventory_id: &str,
    snapshot: BluefsReplaySnapshot,
) -> CephBluefsReplayAggregate {
    let replay = CephBluefsReplayRecord {
        inventory_id: inventory_id.to_string(),
        transaction_count: snapshot.transaction_count,
        first_sequence: snapshot.first_sequence,
        final_sequence: snapshot.final_sequence,
        logical_bytes: snapshot.logical_bytes,
        stop_reason: snapshot.stop_reason,
    };
    let directories = snapshot
        .directories
        .into_iter()
        .map(|path| CephBluefsDirectoryRecord {
            inventory_id: inventory_id.to_string(),
            path,
        })
        .collect();
    let mut files = Vec::with_capacity(snapshot.files.len());
    let mut file_extents = Vec::new();
    for file in snapshot.files {
        for (ordinal, extent) in file.fnode.extents.iter().enumerate() {
            file_extents.push(CephBluefsFileExtentRecord {
                inventory_id: inventory_id.to_string(),
                file_path: file.path.clone(),
                ordinal: ordinal as u32,
                device_id: extent.bdev,
                offset: extent.offset,
                length: extent.length,
            });
        }
        files.push(CephBluefsFileRecord {
            inventory_id: inventory_id.to_string(),
            path: file.path,
            inode: file.inode,
            size: file.fnode.size,
            mtime_seconds: i64::from(file.fnode.mtime.seconds),
            mtime_nanoseconds: file.fnode.mtime.nanoseconds,
            encoding: file.fnode.encoding,
            content_size: file.fnode.content_size,
        });
    }
    CephBluefsReplayAggregate {
        replay,
        directories,
        files,
        file_extents,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_bluefs_records.rs"]
mod tests;
