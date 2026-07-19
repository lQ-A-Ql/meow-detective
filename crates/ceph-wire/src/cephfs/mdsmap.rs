use std::collections::{BTreeMap, BTreeSet};

use crate::{
    codec::{decode_string, CephDecode, CephUtime},
    CephCursor, CephWireError, Result,
};

use super::{
    types::{CephMdsDaemon, CephMdsMap, CephMdsState},
    wire::{
        decode_bool, decode_count, decode_envelope, decode_i32_set, decode_i32_u64_map, invalid,
        skip_compat_set, skip_i32_i32_map, MAX_CEPHFS_MDS_DAEMONS, MAX_CEPHFS_NAME_BYTES,
        MAX_CEPHFS_POOLS,
    },
};

const MDS_MAP_NAME: &str = "MDSMap";
const MDS_MAP_MIN_VERSION: u8 = 4;
const MDS_MAP_DECODER_VERSION: u8 = 5;
const MDS_INFO_MIN_VERSION: u8 = 4;
const MDS_INFO_DECODER_VERSION: u8 = 10;

pub fn decode_ceph_mds_map(input: &[u8]) -> Result<CephMdsMap> {
    let mut cursor = CephCursor::new(input);
    let map = decode_mds_map(&mut cursor)?;
    if !cursor.is_empty() {
        return Err(CephWireError::CephFsTrailingBytes {
            map: MDS_MAP_NAME,
            remaining: cursor.remaining(),
        });
    }
    Ok(map)
}

fn decode_mds_map(cursor: &mut CephCursor<'_>) -> Result<CephMdsMap> {
    let (envelope, mut payload) = decode_envelope(
        cursor,
        MDS_MAP_NAME,
        MDS_MAP_MIN_VERSION,
        MDS_MAP_DECODER_VERSION,
    )?;
    let epoch = u32::decode(&mut payload)?;
    u32::decode(&mut payload)?; // flags
    u32::decode(&mut payload)?; // last_failure
    i32::decode(&mut payload)?; // root rank
    u32::decode(&mut payload)?; // session_timeout
    u32::decode(&mut payload)?; // session_autoclose
    u64::decode(&mut payload)?; // max_file_size
    let max_mds = i32::decode(&mut payload)?;
    let daemons = decode_daemons(&mut payload)?;
    let data_pool_ids = decode_data_pools(&mut payload)?;
    i64::decode(&mut payload)?; // legacy CAS pool

    let extended_version = if envelope.version >= 2 {
        u16::decode(&mut payload)?
    } else {
        1
    };
    if extended_version >= 3 {
        skip_compat_set(&mut payload)?;
    }
    let metadata_pool_id = if extended_version < 5 {
        i64::from(u32::decode(&mut payload)?)
    } else {
        i64::decode(&mut payload)?
    };
    CephUtime::decode(&mut payload)?; // created
    CephUtime::decode(&mut payload)?; // modified
    i32::decode(&mut payload)?; // tableserver
    let in_ranks = decode_i32_set(&mut payload, "MDSMap in ranks")?;
    skip_i32_i32_map(&mut payload, "MDSMap rank incarnations")?;
    let up_ranks = decode_i32_u64_map(&mut payload, "MDSMap up ranks")?;
    let failed_ranks = decode_i32_set(&mut payload, "MDSMap failed ranks")?;
    let stopped_ranks = decode_i32_set(&mut payload, "MDSMap stopped ranks")?;
    let last_failure_osd_epoch = if extended_version >= 4 {
        u32::decode(&mut payload)?
    } else {
        0
    };
    skip_feature_flags(&mut payload, extended_version)?;
    let (enabled, name) = if extended_version >= 8 {
        (
            decode_bool(&mut payload, MDS_MAP_NAME, "enabled")?,
            decode_string(&mut payload, MAX_CEPHFS_NAME_BYTES, "CephFS name")?,
        )
    } else {
        (epoch > 1, "cephfs".to_string())
    };
    let damaged_ranks = if extended_version >= 9 {
        decode_i32_set(&mut payload, "MDSMap damaged ranks")?
    } else {
        BTreeSet::new()
    };
    decode_extended_tail(&mut payload, extended_version)?;
    if !payload.is_empty() {
        return Err(CephWireError::CephFsTrailingBytes {
            map: MDS_MAP_NAME,
            remaining: payload.remaining(),
        });
    }

    let map = CephMdsMap {
        epoch,
        name,
        enabled,
        metadata_pool_id,
        data_pool_ids,
        max_mds,
        last_failure_osd_epoch,
        daemons,
        in_ranks,
        up_ranks,
        failed_ranks,
        stopped_ranks,
        damaged_ranks,
    };
    validate_mds_map(&map)?;
    Ok(map)
}

fn decode_daemons(cursor: &mut CephCursor<'_>) -> Result<Vec<CephMdsDaemon>> {
    let count = decode_count(cursor, MAX_CEPHFS_MDS_DAEMONS, "MDSMap daemons")?;
    let mut daemons = BTreeMap::new();
    for _ in 0..count {
        let map_gid = u64::decode(cursor)?;
        let daemon = decode_daemon(cursor)?;
        if daemon.gid != map_gid {
            return Err(invalid(
                MDS_MAP_NAME,
                "mds_info.gid",
                "map key and encoded daemon GID differ",
            ));
        }
        if daemons.insert(map_gid, daemon).is_some() {
            let value = i64::try_from(map_gid).unwrap_or(i64::MAX);
            return Err(CephWireError::DuplicateCephFsIdentifier {
                kind: "MDS daemon GID",
                value,
            });
        }
    }
    Ok(daemons.into_values().collect())
}

pub(super) fn decode_daemon(cursor: &mut CephCursor<'_>) -> Result<CephMdsDaemon> {
    let (envelope, mut payload) = decode_envelope(
        cursor,
        "mds_info_t",
        MDS_INFO_MIN_VERSION,
        MDS_INFO_DECODER_VERSION,
    )?;
    let gid = u64::decode(&mut payload)?;
    let name = decode_string(
        &mut payload,
        MAX_CEPHFS_NAME_BYTES,
        "CephFS MDS daemon name",
    )?;
    let rank = i32::decode(&mut payload)?;
    let incarnation = i32::decode(&mut payload)?;
    let state = CephMdsState::from_raw(i32::decode(&mut payload)?)?;
    let state_sequence = u64::decode(&mut payload)?;
    skip_entity_addrvec(&mut payload)?;
    CephUtime::decode(&mut payload)?; // laggy_since
    i32::decode(&mut payload)?; // legacy standby_for_rank
    decode_string(
        &mut payload,
        MAX_CEPHFS_NAME_BYTES,
        "CephFS legacy standby daemon name",
    )?;
    if envelope.version >= 2 {
        decode_i32_set(&mut payload, "MDS export targets")?;
    }
    if envelope.version >= 5 {
        u64::decode(&mut payload)?; // MDS feature mask
    }
    if envelope.version >= 6 {
        i64::decode(&mut payload)?; // join filesystem ID
    }
    if envelope.version >= 7 {
        decode_bool(&mut payload, "mds_info_t", "standby replay")?;
    }
    if envelope.version >= 9 {
        u64::decode(&mut payload)?; // daemon flags
    }
    if envelope.version >= 10 {
        skip_compat_set(&mut payload)?;
    }
    if !payload.is_empty() {
        return Err(CephWireError::CephFsTrailingBytes {
            map: "mds_info_t",
            remaining: payload.remaining(),
        });
    }
    if name.trim().is_empty() || name.contains('\0') {
        return Err(invalid(
            MDS_MAP_NAME,
            "mds_info.name",
            "daemon name is empty or contains NUL",
        ));
    }
    Ok(CephMdsDaemon {
        gid,
        name,
        rank,
        incarnation,
        state,
        state_sequence,
    })
}

fn skip_entity_addrvec(cursor: &mut CephCursor<'_>) -> Result<()> {
    match u8::decode(cursor)? {
        0 => {
            cursor.skip(3)?; // remaining bytes of the legacy u32 marker
            u32::decode(cursor)?; // nonce
            cursor.skip(128)?; // encoded Linux sockaddr_storage
            Ok(())
        }
        1 => skip_entity_address_payload(cursor),
        2 => {
            let count = decode_count(cursor, 64, "MDS address vector")?;
            for _ in 0..count {
                skip_entity_address(cursor)?;
            }
            Ok(())
        }
        _ => Err(invalid(
            MDS_MAP_NAME,
            "mds_info.addrs",
            "unsupported entity address vector marker",
        )),
    }
}

fn skip_entity_address(cursor: &mut CephCursor<'_>) -> Result<()> {
    match u8::decode(cursor)? {
        0 => {
            cursor.skip(3)?;
            u32::decode(cursor)?;
            cursor.skip(128)?;
            Ok(())
        }
        1 => skip_entity_address_payload(cursor),
        _ => Err(invalid(
            MDS_MAP_NAME,
            "mds_info.address",
            "unsupported entity address marker",
        )),
    }
}

fn skip_entity_address_payload(cursor: &mut CephCursor<'_>) -> Result<()> {
    let (_, mut payload) = decode_envelope(cursor, "entity_addr_t", 1, 1)?;
    u32::decode(&mut payload)?; // address type
    u32::decode(&mut payload)?; // nonce
    let length = u32::decode(&mut payload)? as usize;
    if length > 128 {
        return Err(CephWireError::LengthLimit {
            context: "Ceph entity address",
            length,
            limit: 128,
        });
    }
    payload.skip(length)?;
    if !payload.is_empty() {
        return Err(CephWireError::CephFsTrailingBytes {
            map: "entity_addr_t",
            remaining: payload.remaining(),
        });
    }
    Ok(())
}

fn decode_data_pools(cursor: &mut CephCursor<'_>) -> Result<Vec<i64>> {
    let count = decode_count(cursor, MAX_CEPHFS_POOLS, "CephFS data pools")?;
    let mut seen = BTreeSet::new();
    let mut pools = Vec::with_capacity(count);
    for _ in 0..count {
        let pool = i64::decode(cursor)?;
        if !seen.insert(pool) {
            return Err(CephWireError::DuplicateCephFsIdentifier {
                kind: "data pool",
                value: pool,
            });
        }
        pools.push(pool);
    }
    Ok(pools)
}

fn skip_feature_flags(cursor: &mut CephCursor<'_>, extended_version: u16) -> Result<()> {
    if extended_version >= 6 {
        if extended_version < 10 {
            decode_bool(cursor, MDS_MAP_NAME, "ever allowed snapshots")?;
            decode_bool(cursor, MDS_MAP_NAME, "explicitly allowed snapshots")?;
        } else {
            u8::decode(cursor)?;
            u8::decode(cursor)?;
        }
    }
    if extended_version >= 7 {
        decode_bool(cursor, MDS_MAP_NAME, "inline data enabled")?;
    }
    Ok(())
}

fn decode_extended_tail(cursor: &mut CephCursor<'_>, extended_version: u16) -> Result<()> {
    if extended_version > 19 {
        return Err(invalid(
            MDS_MAP_NAME,
            "extended_version",
            "extended encoding version is newer than the reviewed layout",
        ));
    }
    if extended_version >= 11 {
        decode_string(cursor, MAX_CEPHFS_NAME_BYTES, "CephFS balancer")?;
    }
    if extended_version >= 12 {
        i32::decode(cursor)?; // standby count wanted
    }
    if extended_version >= 13 {
        i32::decode(cursor)?; // old max MDS
    }
    if extended_version >= 14 {
        u8::decode(cursor)?; // minimum compatible client release
    }
    if extended_version >= 15 {
        let length = u32::decode(cursor)? as usize;
        if length > 4 * 1024 {
            return Err(CephWireError::LengthLimit {
                context: "CephFS required client features",
                length,
                limit: 4 * 1024,
            });
        }
        cursor.skip(length)?;
    }
    if extended_version >= 16 {
        decode_string(cursor, MAX_CEPHFS_NAME_BYTES, "CephFS balancer rank mask")?;
    }
    if extended_version >= 17 {
        u64::decode(cursor)?; // max xattr size
    }
    if extended_version >= 18 {
        u64::decode(cursor)?; // quiesce DB cluster leader
    }
    if extended_version >= 19 {
        let count = decode_count(cursor, MAX_CEPHFS_MDS_DAEMONS, "quiesce DB members")?;
        let mut members = BTreeSet::new();
        for _ in 0..count {
            let gid = u64::decode(cursor)?;
            if !members.insert(gid) {
                return Err(CephWireError::DuplicateCephFsIdentifier {
                    kind: "quiesce DB member GID",
                    value: i64::try_from(gid).unwrap_or(i64::MAX),
                });
            }
        }
    }
    Ok(())
}

fn validate_mds_map(map: &CephMdsMap) -> Result<()> {
    if map.epoch == 0 {
        return Err(invalid(MDS_MAP_NAME, "epoch", "epoch must be non-zero"));
    }
    if map.name.trim().is_empty() || map.name.contains('\0') {
        return Err(invalid(
            MDS_MAP_NAME,
            "name",
            "filesystem name is empty or contains NUL",
        ));
    }
    if map.max_mds <= 0 {
        return Err(invalid(
            MDS_MAP_NAME,
            "max_mds",
            "maximum MDS rank count must be positive",
        ));
    }
    if map.metadata_pool_id <= 0 {
        return Err(invalid(
            MDS_MAP_NAME,
            "metadata_pool",
            "metadata pool ID must be positive",
        ));
    }
    if map.data_pool_ids.is_empty() || map.data_pool_ids.iter().any(|pool| *pool <= 0) {
        return Err(invalid(
            MDS_MAP_NAME,
            "data_pools",
            "at least one positive data pool ID is required",
        ));
    }
    if map.data_pool_ids.contains(&map.metadata_pool_id) {
        return Err(invalid(
            MDS_MAP_NAME,
            "pool bindings",
            "metadata and data pools must be distinct",
        ));
    }
    validate_rank_sets(map)
}

fn validate_rank_sets(map: &CephMdsMap) -> Result<()> {
    if map.in_ranks.iter().any(|rank| *rank < 0)
        || map.up_ranks.keys().any(|rank| *rank < 0)
        || map.failed_ranks.iter().any(|rank| *rank < 0)
        || map.stopped_ranks.iter().any(|rank| *rank < 0)
        || map.damaged_ranks.iter().any(|rank| *rank < 0)
    {
        return Err(invalid(
            MDS_MAP_NAME,
            "rank sets",
            "filesystem rank identifiers must be non-negative",
        ));
    }
    if !map.failed_ranks.is_disjoint(&map.stopped_ranks)
        || !map.failed_ranks.is_disjoint(&map.damaged_ranks)
        || !map.stopped_ranks.is_disjoint(&map.damaged_ranks)
    {
        return Err(invalid(
            MDS_MAP_NAME,
            "rank sets",
            "failed, stopped, and damaged rank sets overlap",
        ));
    }

    let daemon_by_gid = map
        .daemons
        .iter()
        .map(|daemon| (daemon.gid, daemon))
        .collect::<BTreeMap<_, _>>();
    for (rank, gid) in &map.up_ranks {
        if !map.in_ranks.contains(rank) {
            return Err(invalid(
                MDS_MAP_NAME,
                "up ranks",
                "an up rank is absent from the in set",
            ));
        }
        let Some(daemon) = daemon_by_gid.get(gid) else {
            return Err(invalid(
                MDS_MAP_NAME,
                "up ranks",
                "an up rank references a missing daemon GID",
            ));
        };
        if daemon.rank != *rank {
            return Err(invalid(
                MDS_MAP_NAME,
                "up ranks",
                "daemon rank does not match the up-map rank",
            ));
        }
    }
    Ok(())
}
