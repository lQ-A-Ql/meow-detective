use crate::error::{CephWireError, Result};

const RAW_PREFIX_LENGTH: usize = 2;
const BULK_FIXED_LENGTH: usize = 8;
const PER_POOL_FIXED_LENGTH: usize = 16;
const PER_PG_FIXED_LENGTH: usize = 20;

/// BlueStore RocksDB key family used for OMAP records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlueStoreOmapKeyFamily {
    Bulk,
    PgMeta,
    PerPool,
    PerPg,
}

impl BlueStoreOmapKeyFamily {
    pub const fn prefix_byte(self) -> u8 {
        match self {
            Self::Bulk => b'M',
            Self::PgMeta => b'P',
            Self::PerPool => b'm',
            Self::PerPg => b'p',
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Bulk => "bulk (M)",
            Self::PgMeta => "pgmeta (P)",
            Self::PerPool => "per-pool (m)",
            Self::PerPg => "per-pg (p)",
        }
    }
}

impl TryFrom<u8> for BlueStoreOmapKeyFamily {
    type Error = CephWireError;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            b'M' => Ok(Self::Bulk),
            b'P' => Ok(Self::PgMeta),
            b'm' => Ok(Self::PerPool),
            b'p' => Ok(Self::PerPg),
            _ => Err(invalid_raw_key("unknown OMAP family prefix")),
        }
    }
}

/// Pool identifier encoding carried by an OMAP key family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlueStoreOmapPool {
    PerPool(i64),
    PerPg(u64),
}

/// Position of an OMAP key inside the header-entry-tail range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlueStoreOmapKeyKind<'a> {
    Header,
    Entry { user_key: &'a [u8] },
    Tail,
}

impl<'a> BlueStoreOmapKeyKind<'a> {
    pub const fn marker(self) -> u8 {
        match self {
            Self::Header => b'-',
            Self::Entry { .. } => b'.',
            Self::Tail => b'~',
        }
    }

    pub const fn user_key(self) -> Option<&'a [u8]> {
        match self {
            Self::Entry { user_key } => Some(user_key),
            Self::Header | Self::Tail => None,
        }
    }
}

/// Strictly decoded BlueStore OMAP logical key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreOmapKey<'a> {
    pub family: BlueStoreOmapKeyFamily,
    pub pool: Option<BlueStoreOmapPool>,
    pub hash: Option<u32>,
    pub nid: u64,
    pub kind: BlueStoreOmapKeyKind<'a>,
}

impl<'a> BlueStoreOmapKey<'a> {
    pub const fn user_key(&self) -> Option<&'a [u8]> {
        self.kind.user_key()
    }
}

/// Decode a logical key after its RocksDB key-space prefix has been removed.
pub fn decode_bluestore_omap_key<'a>(
    family: BlueStoreOmapKeyFamily,
    logical_key: &'a [u8],
) -> Result<BlueStoreOmapKey<'a>> {
    let fixed_length = fixed_length(family);
    if logical_key.len() <= fixed_length {
        return Err(invalid_key(
            family,
            "key is truncated before the canonical kind marker",
        ));
    }

    let (pool, hash, nid_offset) = decode_scope(family, logical_key)?;
    let nid = read_be_u64(logical_key, nid_offset, family)?;
    let marker = logical_key[fixed_length];
    let trailing = &logical_key[fixed_length + 1..];
    let kind = decode_kind(family, marker, trailing)?;

    Ok(BlueStoreOmapKey {
        family,
        pool,
        hash,
        nid,
        kind,
    })
}

/// Decode a default-column-family key encoded as `family + NUL + logical key`.
pub fn decode_bluestore_raw_omap_key(raw_key: &[u8]) -> Result<BlueStoreOmapKey<'_>> {
    if raw_key.len() < RAW_PREFIX_LENGTH {
        return Err(invalid_raw_key(
            "key is truncated before the family and NUL separator",
        ));
    }
    let family = BlueStoreOmapKeyFamily::try_from(raw_key[0])?;
    if raw_key[1] != 0 {
        return Err(invalid_key(
            family,
            "family prefix is not followed by the canonical NUL separator",
        ));
    }
    decode_bluestore_omap_key(family, &raw_key[RAW_PREFIX_LENGTH..])
}

/// Alias that makes the logical-key input contract explicit at call sites.
pub fn decode_bluestore_omap_logical_key<'a>(
    family: BlueStoreOmapKeyFamily,
    logical_key: &'a [u8],
) -> Result<BlueStoreOmapKey<'a>> {
    decode_bluestore_omap_key(family, logical_key)
}

fn fixed_length(family: BlueStoreOmapKeyFamily) -> usize {
    match family {
        BlueStoreOmapKeyFamily::Bulk | BlueStoreOmapKeyFamily::PgMeta => BULK_FIXED_LENGTH,
        BlueStoreOmapKeyFamily::PerPool => PER_POOL_FIXED_LENGTH,
        BlueStoreOmapKeyFamily::PerPg => PER_PG_FIXED_LENGTH,
    }
}

fn decode_scope(
    family: BlueStoreOmapKeyFamily,
    key: &[u8],
) -> Result<(Option<BlueStoreOmapPool>, Option<u32>, usize)> {
    match family {
        BlueStoreOmapKeyFamily::Bulk | BlueStoreOmapKeyFamily::PgMeta => Ok((None, None, 0)),
        BlueStoreOmapKeyFamily::PerPool => {
            let pool = read_be_i64(key, 0, family)?;
            Ok((Some(BlueStoreOmapPool::PerPool(pool)), None, 8))
        }
        BlueStoreOmapKeyFamily::PerPg => {
            let pool = read_be_u64(key, 0, family)?;
            let hash = read_be_u32(key, 8, family)?;
            Ok((Some(BlueStoreOmapPool::PerPg(pool)), Some(hash), 12))
        }
    }
}

fn decode_kind<'a>(
    family: BlueStoreOmapKeyFamily,
    marker: u8,
    trailing: &'a [u8],
) -> Result<BlueStoreOmapKeyKind<'a>> {
    match marker {
        b'-' if trailing.is_empty() => Ok(BlueStoreOmapKeyKind::Header),
        b'-' => Err(invalid_key(
            family,
            "bytes follow the canonical header marker",
        )),
        b'.' => Ok(BlueStoreOmapKeyKind::Entry { user_key: trailing }),
        b'~' if trailing.is_empty() => Ok(BlueStoreOmapKeyKind::Tail),
        b'~' => Err(invalid_key(
            family,
            "bytes follow the canonical tail marker",
        )),
        _ => Err(invalid_key(
            family,
            "expected the canonical header, entry, or tail marker",
        )),
    }
}

fn read_be_u32(key: &[u8], offset: usize, family: BlueStoreOmapKeyFamily) -> Result<u32> {
    let bytes = read_fixed::<4>(key, offset, family, "sortable u32 is truncated")?;
    Ok(u32::from_be_bytes(bytes))
}

fn read_be_u64(key: &[u8], offset: usize, family: BlueStoreOmapKeyFamily) -> Result<u64> {
    let bytes = read_fixed::<8>(key, offset, family, "sortable u64 is truncated")?;
    Ok(u64::from_be_bytes(bytes))
}

fn read_be_i64(key: &[u8], offset: usize, family: BlueStoreOmapKeyFamily) -> Result<i64> {
    let bytes = read_fixed::<8>(key, offset, family, "sortable i64 is truncated")?;
    Ok(i64::from_be_bytes(bytes))
}

fn read_fixed<const N: usize>(
    key: &[u8],
    offset: usize,
    family: BlueStoreOmapKeyFamily,
    reason: &'static str,
) -> Result<[u8; N]> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| invalid_key(family, reason))?;
    key.get(offset..end)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| invalid_key(family, reason))
}

fn invalid_key(family: BlueStoreOmapKeyFamily, reason: &'static str) -> CephWireError {
    CephWireError::InvalidBlueStoreOmapKey {
        family: family.label(),
        reason,
    }
}

fn invalid_raw_key(reason: &'static str) -> CephWireError {
    CephWireError::InvalidBlueStoreOmapKey {
        family: "raw",
        reason,
    }
}
