use crate::io::{read_exact_at, read_u16, read_u32, SharedReader};
use crate::{ErofsError, Result};

use super::{
    head_kind, IndexEntry, LCLUSTER_D0_CBLKCNT, LCLUSTER_TYPE_MASK, LCLUSTER_TYPE_NONHEAD,
};

pub(super) fn read_entry(
    source: &SharedReader,
    index_offset: u64,
    index: u64,
) -> Result<IndexEntry> {
    let offset = index
        .checked_mul(super::FULL_INDEX_BYTES)
        .and_then(|relative| index_offset.checked_add(relative))
        .ok_or_else(|| ErofsError::Invalid("compression index address overflows".to_string()))?;
    let bytes = read_exact_at(source, offset, super::FULL_INDEX_BYTES as usize)?;
    let advise = read_u16(&bytes, 0, "compressed cluster advice")?;
    let kind = advise & LCLUSTER_TYPE_MASK;
    if kind == LCLUSTER_TYPE_NONHEAD {
        if advise != LCLUSTER_TYPE_NONHEAD {
            return Err(ErofsError::Invalid(
                "NONHEAD compression entry has head-only flags".to_string(),
            ));
        }
        let delta_back = read_u16(&bytes, 4, "compressed lookback distance")?;
        if delta_back == 0 {
            return Err(ErofsError::Invalid(
                "compressed lookback distance is zero".to_string(),
            ));
        }
        if delta_back & LCLUSTER_D0_CBLKCNT != 0 {
            return Err(ErofsError::Unsupported(
                "big physical compression clusters".to_string(),
            ));
        }
        return Ok(IndexEntry::NonHead {
            delta_back,
            delta_forward: read_u16(&bytes, 6, "compressed lookahead distance")?,
        });
    }
    if advise & !(LCLUSTER_TYPE_MASK | super::LCLUSTER_HOLE) != 0 {
        return Err(ErofsError::Unsupported(format!(
            "compressed cluster advice flags {:#x}",
            advise & !(LCLUSTER_TYPE_MASK | super::LCLUSTER_HOLE)
        )));
    }
    Ok(IndexEntry::Head {
        kind: if advise & super::LCLUSTER_HOLE != 0 {
            super::HeadKind::Hole
        } else {
            head_kind(kind)?
        },
        cluster_offset: read_u16(&bytes, 2, "compressed cluster offset")?,
        block: u64::from(read_u32(&bytes, 4, "compressed cluster block")?),
    })
}

pub(super) fn validate_table(
    index_offset: u64,
    size: u64,
    block_size: usize,
    volume_offset: u64,
    block_count: u64,
) -> Result<()> {
    let index_bytes = size
        .div_ceil(block_size as u64)
        .checked_mul(super::FULL_INDEX_BYTES)
        .ok_or_else(|| ErofsError::Invalid("compression index table overflows".to_string()))?;
    super::validate_metadata_end(
        index_offset,
        index_bytes,
        block_size,
        volume_offset,
        block_count,
    )
}
