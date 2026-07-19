use std::collections::{BTreeMap, BTreeSet};

use crate::{
    codec::{decode_string, CephDecode, CephStructEnvelope},
    CephCursor, CephWireError, Result,
};

pub(super) const MAX_CEPHFS_FILESYSTEMS: usize = 128;
pub(super) const MAX_CEPHFS_MDS_DAEMONS: usize = 4_096;
pub(super) const MAX_CEPHFS_POOLS: usize = 4_096;
pub(super) const MAX_CEPHFS_RANKS: usize = 4_096;
pub(super) const MAX_CEPHFS_MAP_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_CEPHFS_NAME_BYTES: usize = 4 * 1024;
const MAX_COMPAT_FEATURES: usize = 64;

pub(super) fn decode_count(
    cursor: &mut CephCursor<'_>,
    limit: usize,
    context: &'static str,
) -> Result<usize> {
    let count = u32::decode(cursor)? as usize;
    if count > limit {
        return Err(CephWireError::LengthLimit {
            context,
            length: count,
            limit,
        });
    }
    Ok(count)
}

pub(super) fn decode_bool(
    cursor: &mut CephCursor<'_>,
    map: &'static str,
    field: &'static str,
) -> Result<bool> {
    match u8::decode(cursor)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid(map, field, "boolean wire value is not zero or one")),
    }
}

pub(super) fn decode_buffer<'a>(
    cursor: &mut CephCursor<'a>,
    context: &'static str,
) -> Result<&'a [u8]> {
    let length = u32::decode(cursor)? as usize;
    if length > MAX_CEPHFS_MAP_BYTES {
        return Err(CephWireError::LengthLimit {
            context,
            length,
            limit: MAX_CEPHFS_MAP_BYTES,
        });
    }
    cursor.read_exact(length)
}

pub(super) fn decode_i32_set(
    cursor: &mut CephCursor<'_>,
    context: &'static str,
) -> Result<BTreeSet<i32>> {
    let count = decode_count(cursor, MAX_CEPHFS_RANKS, context)?;
    let mut values = BTreeSet::new();
    for _ in 0..count {
        let value = i32::decode(cursor)?;
        if !values.insert(value) {
            return Err(CephWireError::DuplicateCephFsIdentifier {
                kind: context,
                value: i64::from(value),
            });
        }
    }
    Ok(values)
}

pub(super) fn decode_i32_u64_map(
    cursor: &mut CephCursor<'_>,
    context: &'static str,
) -> Result<BTreeMap<i32, u64>> {
    let count = decode_count(cursor, MAX_CEPHFS_RANKS, context)?;
    let mut values = BTreeMap::new();
    for _ in 0..count {
        let key = i32::decode(cursor)?;
        let value = u64::decode(cursor)?;
        if values.insert(key, value).is_some() {
            return Err(CephWireError::DuplicateCephFsIdentifier {
                kind: context,
                value: i64::from(key),
            });
        }
    }
    Ok(values)
}

pub(super) fn skip_i32_i32_map(cursor: &mut CephCursor<'_>, context: &'static str) -> Result<()> {
    let count = decode_count(cursor, MAX_CEPHFS_RANKS, context)?;
    for _ in 0..count {
        i32::decode(cursor)?;
        i32::decode(cursor)?;
    }
    Ok(())
}

pub(super) fn skip_compat_set(cursor: &mut CephCursor<'_>) -> Result<()> {
    for _ in 0..3 {
        u64::decode(cursor)?;
        let count = decode_count(cursor, MAX_COMPAT_FEATURES, "Ceph compat features")?;
        for _ in 0..count {
            u64::decode(cursor)?;
            decode_string(cursor, MAX_CEPHFS_NAME_BYTES, "Ceph compat feature name")?;
        }
    }
    Ok(())
}

pub(super) fn decode_envelope<'a>(
    cursor: &mut CephCursor<'a>,
    map: &'static str,
    minimum_version: u8,
    decoder_version: u8,
) -> Result<(CephStructEnvelope, CephCursor<'a>)> {
    let (envelope, payload) = CephStructEnvelope::decode_payload(cursor, decoder_version)?;
    if envelope.version < minimum_version || envelope.version > decoder_version {
        return Err(CephWireError::UnsupportedCephFsMapVersion {
            map,
            encoded_version: envelope.version,
            minimum_version,
            decoder_version,
        });
    }
    Ok((envelope, payload))
}

pub(super) fn invalid(
    map: &'static str,
    field: &'static str,
    reason: &'static str,
) -> CephWireError {
    CephWireError::InvalidCephFsMap { map, field, reason }
}
