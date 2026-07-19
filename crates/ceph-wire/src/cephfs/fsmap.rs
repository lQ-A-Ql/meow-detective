use std::collections::BTreeSet;

use crate::{
    codec::{decode_string, CephDecode, CephUtime},
    CephCursor, CephWireError, Result,
};

use super::{
    mdsmap::{decode_ceph_mds_map, decode_daemon},
    types::{CephFsFilesystem, CephFsMap},
    wire::{
        decode_bool, decode_buffer, decode_count, decode_envelope, invalid, skip_compat_set,
        MAX_CEPHFS_FILESYSTEMS, MAX_CEPHFS_MDS_DAEMONS, MAX_CEPHFS_NAME_BYTES,
    },
};

const FS_MAP_NAME: &str = "FSMap";
const FS_MAP_MIN_VERSION: u8 = 7;
const FS_MAP_DECODER_VERSION: u8 = 8;
const FILESYSTEM_DECODER_VERSION: u8 = 2;
const MAX_MIRROR_PEERS: usize = 4_096;

pub fn decode_ceph_fs_map(input: &[u8]) -> Result<CephFsMap> {
    let mut cursor = CephCursor::new(input);
    let (_, mut payload) = decode_envelope(
        &mut cursor,
        FS_MAP_NAME,
        FS_MAP_MIN_VERSION,
        FS_MAP_DECODER_VERSION,
    )?;
    if !cursor.is_empty() {
        return Err(CephWireError::CephFsTrailingBytes {
            map: FS_MAP_NAME,
            remaining: cursor.remaining(),
        });
    }

    let epoch = u32::decode(&mut payload)?;
    i64::decode(&mut payload)?; // next_filesystem_id
    i64::decode(&mut payload)?; // legacy_client_fscid
    skip_compat_set(&mut payload)?;
    decode_bool(&mut payload, FS_MAP_NAME, "enable_multiple")?;
    let count = decode_count(&mut payload, MAX_CEPHFS_FILESYSTEMS, "CephFS filesystems")?;
    let mut seen = BTreeSet::new();
    let mut filesystems = Vec::with_capacity(count);
    for _ in 0..count {
        let filesystem = decode_filesystem(&mut payload)?;
        if !seen.insert(filesystem.filesystem_id) {
            return Err(CephWireError::DuplicateCephFsIdentifier {
                kind: "filesystem",
                value: filesystem.filesystem_id,
            });
        }
        if filesystem.mds_map.epoch != epoch {
            return Err(invalid(
                FS_MAP_NAME,
                "filesystem.mds_map.epoch",
                "nested MDSMap epoch differs from the FSMap epoch",
            ));
        }
        filesystems.push(filesystem);
    }
    decode_fsmap_tail(&mut payload)?;
    if epoch == 0 {
        return Err(invalid(FS_MAP_NAME, "epoch", "epoch must be non-zero"));
    }
    filesystems.sort_by_key(|filesystem| filesystem.filesystem_id);
    Ok(CephFsMap { epoch, filesystems })
}

fn decode_fsmap_tail(payload: &mut CephCursor<'_>) -> Result<()> {
    let role_count = decode_count(payload, MAX_CEPHFS_MDS_DAEMONS, "FSMap MDS roles")?;
    let mut role_gids = BTreeSet::new();
    for _ in 0..role_count {
        let gid = u64::decode(payload)?;
        i64::decode(payload)?;
        if !role_gids.insert(gid) {
            return Err(CephWireError::DuplicateCephFsIdentifier {
                kind: "FSMap MDS role GID",
                value: i64::try_from(gid).unwrap_or(i64::MAX),
            });
        }
    }

    let standby_count = decode_count(payload, MAX_CEPHFS_MDS_DAEMONS, "FSMap standby daemons")?;
    let mut standby_gids = BTreeSet::new();
    for _ in 0..standby_count {
        let gid = u64::decode(payload)?;
        let daemon = decode_daemon(payload)?;
        if daemon.gid != gid || !standby_gids.insert(gid) {
            return Err(invalid(
                FS_MAP_NAME,
                "standby_daemons",
                "standby daemon identity is duplicated or inconsistent",
            ));
        }
    }

    let epoch_count = decode_count(payload, MAX_CEPHFS_MDS_DAEMONS, "FSMap standby epochs")?;
    let mut epoch_gids = BTreeSet::new();
    for _ in 0..epoch_count {
        let gid = u64::decode(payload)?;
        u32::decode(payload)?;
        if !epoch_gids.insert(gid) {
            return Err(CephWireError::DuplicateCephFsIdentifier {
                kind: "FSMap standby epoch GID",
                value: i64::try_from(gid).unwrap_or(i64::MAX),
            });
        }
    }
    decode_bool(payload, FS_MAP_NAME, "ever_enabled_multiple")?;
    CephUtime::decode(payload)?; // btime
    if !payload.is_empty() {
        return Err(CephWireError::CephFsTrailingBytes {
            map: FS_MAP_NAME,
            remaining: payload.remaining(),
        });
    }
    Ok(())
}

fn decode_filesystem(cursor: &mut CephCursor<'_>) -> Result<CephFsFilesystem> {
    let (envelope, mut payload) =
        decode_envelope(cursor, "Filesystem", 1, FILESYSTEM_DECODER_VERSION)?;
    let filesystem_id = i64::decode(&mut payload)?;
    if filesystem_id <= 0 {
        return Err(invalid(
            FS_MAP_NAME,
            "filesystem_id",
            "filesystem ID must be positive",
        ));
    }
    let encoded_mds_map = decode_buffer(&mut payload, "nested MDSMap")?;
    let mds_map = decode_ceph_mds_map(encoded_mds_map)?;
    if envelope.version >= 2 {
        decode_mirror_info(&mut payload)?;
    }
    if envelope.version <= FILESYSTEM_DECODER_VERSION && !payload.is_empty() {
        return Err(CephWireError::CephFsTrailingBytes {
            map: "Filesystem",
            remaining: payload.remaining(),
        });
    }
    Ok(CephFsFilesystem {
        filesystem_id,
        mds_map,
    })
}

fn decode_mirror_info(cursor: &mut CephCursor<'_>) -> Result<()> {
    let (envelope, mut payload) = decode_envelope(cursor, "MirrorInfo", 1, 1)?;
    decode_bool(&mut payload, "MirrorInfo", "mirrored")?;
    let count = decode_count(&mut payload, MAX_MIRROR_PEERS, "CephFS mirror peers")?;
    for _ in 0..count {
        decode_mirror_peer(&mut payload)?;
    }
    ensure_envelope_consumed(envelope.version, 1, payload, "MirrorInfo")
}

fn decode_mirror_peer(cursor: &mut CephCursor<'_>) -> Result<()> {
    let (envelope, mut payload) = decode_envelope(cursor, "MirrorPeer", 1, 1)?;
    decode_string(
        &mut payload,
        MAX_CEPHFS_NAME_BYTES,
        "CephFS mirror peer UUID",
    )?;
    decode_cluster_info(&mut payload)?;
    ensure_envelope_consumed(envelope.version, 1, payload, "MirrorPeer")
}

fn decode_cluster_info(cursor: &mut CephCursor<'_>) -> Result<()> {
    let (envelope, mut payload) = decode_envelope(cursor, "MirrorClusterInfo", 1, 1)?;
    for context in [
        "CephFS mirror client name",
        "CephFS mirror cluster name",
        "CephFS mirror filesystem name",
    ] {
        decode_string(&mut payload, MAX_CEPHFS_NAME_BYTES, context)?;
    }
    ensure_envelope_consumed(envelope.version, 1, payload, "MirrorClusterInfo")
}

fn ensure_envelope_consumed(
    encoded_version: u8,
    decoder_version: u8,
    payload: CephCursor<'_>,
    map: &'static str,
) -> Result<()> {
    if encoded_version <= decoder_version && !payload.is_empty() {
        return Err(CephWireError::CephFsTrailingBytes {
            map,
            remaining: payload.remaining(),
        });
    }
    Ok(())
}
