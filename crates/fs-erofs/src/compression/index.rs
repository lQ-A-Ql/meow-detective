mod compact;
mod full;

use crate::inode::ErofsInode;
use crate::io::{read_exact_at, read_u16, SharedReader};
use crate::{ErofsError, ErofsSuperblock, Result};

use self::compact::CompactIndexLayout;

const MAP_HEADER_BYTES: u64 = 8;
const FULL_INDEX_PADDING: u64 = 8;
const FULL_INDEX_BYTES: u64 = 8;
const COMPACT_PACK_4B_BYTES: u64 = 8;
const COMPACT_PACK_4B_ENTRIES: u64 = 2;
const COMPACT_PACK_2B_BYTES: u64 = 32;
const COMPACT_PACK_2B_ENTRIES: u64 = 16;
const COMPACT_VALUE_BITS: u32 = 14;
const COMPACT_LOW_BITS: u32 = 12;
const LCLUSTER_TYPE_MASK: u16 = 0x0003;
const LCLUSTER_TYPE_PLAIN: u16 = 0;
const LCLUSTER_TYPE_HEAD1: u16 = 1;
const LCLUSTER_TYPE_NONHEAD: u16 = 2;
const LCLUSTER_TYPE_HEAD2: u16 = 3;
const LCLUSTER_HOLE: u16 = 1 << 14;
const LCLUSTER_D0_CBLKCNT: u16 = 1 << 11;
const MAP_HEADER_FRAGMENT: u8 = 1 << 7;
const MAP_ADVISE_COMPACTED_2B: u16 = 0x0001;

pub(super) enum CompressionIndexes {
    Full { offset: u64, entries: u64 },
    Compact(CompactIndexLayout),
}

#[derive(Clone, Copy)]
pub(super) enum HeadKind {
    Plain,
    Lz4,
    Head2,
    Hole,
}

#[derive(Clone, Copy)]
pub(super) enum IndexEntry {
    Head {
        kind: HeadKind,
        cluster_offset: u16,
        block: u64,
    },
    NonHead {
        delta_back: u16,
        delta_forward: u16,
    },
}

impl CompressionIndexes {
    pub(super) fn read(
        source: &SharedReader,
        volume_offset: u64,
        superblock: &ErofsSuperblock,
        inode: &ErofsInode,
    ) -> Result<Self> {
        let metadata_end = inode
            .source_offset
            .checked_add(inode.inode_size as u64)
            .and_then(|offset| offset.checked_add(inode.xattr_size as u64))
            .ok_or_else(|| ErofsError::Invalid("compression map offset overflows".to_string()))?;
        let header_offset = align_up(metadata_end, 8)?;
        let header = read_exact_at(source, header_offset, MAP_HEADER_BYTES as usize)?;
        let advise = validate_map_header(&header, inode.nid, inode.is_compressed_compact())?;
        if inode.is_compressed_compact() {
            return Ok(Self::Compact(CompactIndexLayout::new(
                header_offset,
                advise,
                inode.size,
                superblock,
                volume_offset,
            )?));
        }
        let offset = header_offset
            .checked_add(MAP_HEADER_BYTES + FULL_INDEX_PADDING)
            .ok_or_else(|| ErofsError::Invalid("compression index offset overflows".to_string()))?;
        full::validate_table(
            offset,
            inode.size,
            superblock.block_size,
            volume_offset,
            superblock.block_count,
        )?;
        Ok(Self::Full {
            offset,
            entries: inode.size.div_ceil(superblock.block_size as u64),
        })
    }

    pub(super) fn read_entry(&self, source: &SharedReader, index: u64) -> Result<IndexEntry> {
        if index >= self.entry_count() {
            return Err(ErofsError::Invalid(format!(
                "compression cluster index {index} exceeds file metadata"
            )));
        }
        match self {
            Self::Full { offset, .. } => full::read_entry(source, *offset, index),
            Self::Compact(layout) => compact::read_entry(source, layout, index),
        }
    }

    pub(super) fn entry_count(&self) -> u64 {
        match self {
            Self::Full { entries, .. } => *entries,
            Self::Compact(layout) => layout.entries,
        }
    }
}

fn head_kind(kind: u16) -> Result<HeadKind> {
    match kind {
        LCLUSTER_TYPE_PLAIN => Ok(HeadKind::Plain),
        LCLUSTER_TYPE_HEAD1 => Ok(HeadKind::Lz4),
        LCLUSTER_TYPE_HEAD2 => Ok(HeadKind::Head2),
        _ => Err(ErofsError::Invalid(
            "NONHEAD cluster was decoded as a compression head".to_string(),
        )),
    }
}

fn validate_map_header(bytes: &[u8], nid: u64, compact: bool) -> Result<u16> {
    if read_u16(bytes, 0, "compression map reserved field")? != 0 {
        return Err(ErofsError::Invalid(format!(
            "compression map reserved field is non-zero for inode {nid}"
        )));
    }
    if read_u16(bytes, 2, "compression inline data size")? != 0 {
        return Err(ErofsError::Unsupported(format!(
            "inline compressed tail data for inode {nid}"
        )));
    }
    let advise = read_u16(bytes, 4, "compression map advice")?;
    let allowed_advise = if compact { MAP_ADVISE_COMPACTED_2B } else { 0 };
    if advise & !allowed_advise != 0 {
        return Err(ErofsError::Unsupported(format!(
            "compression map advice {:#x} for inode {nid}",
            advise & !allowed_advise
        )));
    }
    let algorithms = *bytes
        .get(6)
        .ok_or_else(|| ErofsError::Invalid("truncated compression algorithms".to_string()))?;
    if algorithms != 0 {
        return Err(ErofsError::Unsupported(format!(
            "compression algorithms {algorithms:#x} for inode {nid}"
        )));
    }
    let cluster_bits = *bytes
        .get(7)
        .ok_or_else(|| ErofsError::Invalid("truncated compression cluster bits".to_string()))?;
    if cluster_bits & MAP_HEADER_FRAGMENT != 0 {
        return Err(ErofsError::Unsupported(
            "fragment-packed compressed file".to_string(),
        ));
    }
    if cluster_bits != 0 {
        return Err(ErofsError::Unsupported(format!(
            "compression logical cluster bits {cluster_bits:#x}"
        )));
    }
    Ok(advise)
}

fn validate_metadata_end(
    index_offset: u64,
    index_bytes: u64,
    block_size: usize,
    volume_offset: u64,
    block_count: u64,
) -> Result<()> {
    let index_end = index_offset
        .checked_add(index_bytes)
        .ok_or_else(|| ErofsError::Invalid("compression index table overflows".to_string()))?;
    let filesystem_end = block_count
        .checked_mul(block_size as u64)
        .and_then(|bytes| volume_offset.checked_add(bytes))
        .ok_or_else(|| ErofsError::Invalid("filesystem end offset overflows".to_string()))?;
    if index_end > filesystem_end {
        return Err(ErofsError::Invalid(
            "compression index table exceeds filesystem".to_string(),
        ));
    }
    Ok(())
}

fn align_up(value: u64, alignment: u64) -> Result<u64> {
    value
        .checked_add(alignment - 1)
        .map(|sum| sum / alignment * alignment)
        .ok_or_else(|| ErofsError::Invalid("compression map alignment overflows".to_string()))
}
