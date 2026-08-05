use crate::io::SharedReader;
use crate::{ErofsError, Result};

use super::index::{CompressionIndexes, HeadKind, IndexEntry};

const MAX_SINGLE_PCLUSTER_OUTPUT: u64 = 2 * 1024 * 1024;

pub(super) struct MappedExtent {
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) storage: ExtentStorage,
}

pub(super) enum ExtentStorage {
    Plain(u64),
    Lz4(u64),
    Hole,
}

struct HeadEntry {
    lcluster: u64,
    kind: HeadKind,
    cluster_offset: u16,
    block: u64,
}

pub(super) fn map_extent(
    indexes: &CompressionIndexes,
    source: &SharedReader,
    position: u64,
    file_size: u64,
    block_size: usize,
) -> Result<MappedExtent> {
    if position >= file_size {
        return Err(ErofsError::Invalid(
            "compressed mapping starts beyond the file".to_string(),
        ));
    }
    let block_size = block_size as u64;
    let lcluster = position / block_size;
    let entry = indexes.read_entry(source, lcluster)?;
    let head = match entry {
        IndexEntry::Head {
            kind,
            cluster_offset,
            block,
        } if position % block_size >= u64::from(cluster_offset) => HeadEntry {
            lcluster,
            kind,
            cluster_offset,
            block,
        },
        IndexEntry::Head { .. } => find_head(indexes, source, lcluster, 1)?,
        IndexEntry::NonHead { delta_back, .. } => find_head(indexes, source, lcluster, delta_back)?,
    };
    let start = logical_offset(head.lcluster, head.cluster_offset, block_size)?;
    let end = find_extent_end(indexes, source, &head, file_size, block_size)?;
    if start > position || position >= end {
        return Err(ErofsError::Invalid(
            "compressed extent does not contain the requested offset".to_string(),
        ));
    }
    let length = end
        .checked_sub(start)
        .ok_or_else(|| ErofsError::Invalid("compressed extent length underflows".to_string()))?;
    if length > MAX_SINGLE_PCLUSTER_OUTPUT {
        return Err(ErofsError::Unsupported(format!(
            "single-pcluster output of {length} bytes exceeds the bounded decoder"
        )));
    }
    let storage = match head.kind {
        HeadKind::Plain if length <= block_size => ExtentStorage::Plain(head.block),
        HeadKind::Plain => {
            return Err(ErofsError::Invalid(
                "plain compression extent exceeds one physical block".to_string(),
            ))
        }
        HeadKind::Lz4 => ExtentStorage::Lz4(head.block),
        HeadKind::Head2 => {
            return Err(ErofsError::Unsupported(
                "secondary compression algorithms".to_string(),
            ))
        }
        HeadKind::Hole => ExtentStorage::Hole,
    };
    Ok(MappedExtent {
        start,
        end,
        storage,
    })
}

fn find_head(
    indexes: &CompressionIndexes,
    source: &SharedReader,
    start_lcluster: u64,
    initial_distance: u16,
) -> Result<HeadEntry> {
    let mut lcluster = start_lcluster;
    let mut distance = u64::from(initial_distance);
    for _ in 0..indexes.entry_count() {
        if distance == 0 || distance > lcluster {
            return Err(ErofsError::Invalid(
                "compressed lookback distance crosses the file start".to_string(),
            ));
        }
        lcluster -= distance;
        match indexes.read_entry(source, lcluster)? {
            IndexEntry::Head {
                kind,
                cluster_offset,
                block,
            } => {
                return Ok(HeadEntry {
                    lcluster,
                    kind,
                    cluster_offset,
                    block,
                })
            }
            IndexEntry::NonHead { delta_back, .. } => {
                distance = u64::from(delta_back);
            }
        }
    }
    Err(ErofsError::Invalid(
        "compressed lookback chain does not reach a head".to_string(),
    ))
}

fn find_extent_end(
    indexes: &CompressionIndexes,
    source: &SharedReader,
    head: &HeadEntry,
    file_size: u64,
    block_size: u64,
) -> Result<u64> {
    let mut lcluster = head
        .lcluster
        .checked_add(1)
        .ok_or_else(|| ErofsError::Invalid("compressed lookahead overflows".to_string()))?;
    for _ in 0..indexes.entry_count() {
        if lcluster >= indexes.entry_count() {
            return Ok(file_size);
        }
        match indexes.read_entry(source, lcluster)? {
            IndexEntry::Head { cluster_offset, .. } => {
                let end = logical_offset(lcluster, cluster_offset, block_size)?;
                if end > file_size {
                    return Err(ErofsError::Invalid(
                        "compressed extent ends beyond the file".to_string(),
                    ));
                }
                return Ok(end);
            }
            IndexEntry::NonHead { delta_forward, .. } => {
                lcluster = lcluster
                    .checked_add(u64::from(delta_forward.max(1)))
                    .ok_or_else(|| {
                        ErofsError::Invalid("compressed lookahead overflows".to_string())
                    })?;
            }
        }
    }
    Err(ErofsError::Invalid(
        "compressed lookahead chain does not terminate".to_string(),
    ))
}

fn logical_offset(lcluster: u64, cluster_offset: u16, block_size: u64) -> Result<u64> {
    if u64::from(cluster_offset) >= block_size {
        return Err(ErofsError::Invalid(format!(
            "compressed cluster offset {cluster_offset} exceeds its logical cluster"
        )));
    }
    lcluster
        .checked_mul(block_size)
        .and_then(|offset| offset.checked_add(u64::from(cluster_offset)))
        .ok_or_else(|| ErofsError::Invalid("compressed logical offset overflows".to_string()))
}
