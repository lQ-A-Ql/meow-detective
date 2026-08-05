use crate::inode::ErofsInode;
use crate::io::{read_exact_at, read_u16, read_u32, SharedReader};
use crate::{ErofsError, ErofsSuperblock, Result};

use super::ClusterKind;

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
const MAP_HEADER_FRAGMENT: u8 = 1 << 7;
const MAP_ADVISE_COMPACTED_2B: u16 = 0x0001;

pub(super) enum CompressionIndexes {
    Full { offset: u64 },
    Compact(CompactIndexLayout),
}

pub(super) struct CompactIndexLayout {
    offset: u64,
    entries: u64,
    initial_4b: u64,
    compact_2b: u64,
}

struct CompactPackLocation {
    offset: u64,
    bytes: usize,
    entries: usize,
    entry_index: usize,
    encoded_bits: u32,
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
        validate_full_index_table(
            offset,
            inode.size,
            superblock.block_size,
            volume_offset,
            superblock.block_count,
        )?;
        Ok(Self::Full { offset })
    }

    pub(super) fn read_cluster_kind(
        &self,
        source: &SharedReader,
        index: u64,
    ) -> Result<ClusterKind> {
        match self {
            Self::Full { offset } => read_full_cluster_kind(source, *offset, index),
            Self::Compact(layout) => read_compact_cluster_kind(source, layout, index),
        }
    }
}

impl CompactIndexLayout {
    fn new(
        header_offset: u64,
        advise: u16,
        size: u64,
        superblock: &ErofsSuperblock,
        volume_offset: u64,
    ) -> Result<Self> {
        let offset = header_offset
            .checked_add(MAP_HEADER_BYTES)
            .ok_or_else(|| ErofsError::Invalid("compact index offset overflows".to_string()))?;
        let entries = size.div_ceil(superblock.block_size as u64);
        let alignment_entries = ((32 - offset % 32) / 4) & 7;
        let initial_4b = alignment_entries.min(entries);
        let compact_2b = if advise & MAP_ADVISE_COMPACTED_2B != 0 && alignment_entries < entries {
            (entries - alignment_entries) / COMPACT_PACK_2B_ENTRIES * COMPACT_PACK_2B_ENTRIES
        } else {
            0
        };
        let trailing_4b = entries
            .checked_sub(initial_4b)
            .and_then(|remaining| remaining.checked_sub(compact_2b))
            .ok_or_else(|| ErofsError::Invalid("compact index counts underflow".to_string()))?;
        let initial_bytes =
            packed_bytes(initial_4b, COMPACT_PACK_4B_ENTRIES, COMPACT_PACK_4B_BYTES)?;
        let middle_bytes = compact_2b.checked_mul(2).ok_or_else(|| {
            ErofsError::Invalid("compact 2-byte index size overflows".to_string())
        })?;
        let trailing_bytes =
            packed_bytes(trailing_4b, COMPACT_PACK_4B_ENTRIES, COMPACT_PACK_4B_BYTES)?;
        let index_bytes = initial_bytes
            .checked_add(middle_bytes)
            .and_then(|bytes| bytes.checked_add(trailing_bytes))
            .ok_or_else(|| ErofsError::Invalid("compact index table overflows".to_string()))?;
        validate_metadata_end(
            offset,
            index_bytes,
            superblock.block_size,
            volume_offset,
            superblock.block_count,
        )?;
        Ok(Self {
            offset,
            entries,
            initial_4b,
            compact_2b,
        })
    }

    fn locate(&self, index: u64) -> Result<CompactPackLocation> {
        if index >= self.entries {
            return Err(ErofsError::Invalid(format!(
                "compact cluster index {index} exceeds file metadata"
            )));
        }
        if index < self.initial_4b {
            return compact_pack_location(
                self.offset,
                index,
                COMPACT_PACK_4B_ENTRIES,
                COMPACT_PACK_4B_BYTES,
            );
        }
        let after_initial = self
            .offset
            .checked_add(self.initial_4b.checked_mul(4).ok_or_else(|| {
                ErofsError::Invalid("compact initial index offset overflows".to_string())
            })?)
            .ok_or_else(|| {
                ErofsError::Invalid("compact initial index offset overflows".to_string())
            })?;
        let relative = index
            .checked_sub(self.initial_4b)
            .ok_or_else(|| ErofsError::Invalid("compact index position underflows".to_string()))?;
        if relative < self.compact_2b {
            return compact_pack_location(
                after_initial,
                relative,
                COMPACT_PACK_2B_ENTRIES,
                COMPACT_PACK_2B_BYTES,
            );
        }
        let trailing_offset = after_initial
            .checked_add(self.compact_2b.checked_mul(2).ok_or_else(|| {
                ErofsError::Invalid("compact trailing index offset overflows".to_string())
            })?)
            .ok_or_else(|| {
                ErofsError::Invalid("compact trailing index offset overflows".to_string())
            })?;
        compact_pack_location(
            trailing_offset,
            relative.checked_sub(self.compact_2b).ok_or_else(|| {
                ErofsError::Invalid("compact trailing index position underflows".to_string())
            })?,
            COMPACT_PACK_4B_ENTRIES,
            COMPACT_PACK_4B_BYTES,
        )
    }
}

fn read_full_cluster_kind(
    source: &SharedReader,
    index_offset: u64,
    index: u64,
) -> Result<ClusterKind> {
    let offset = index
        .checked_mul(FULL_INDEX_BYTES)
        .and_then(|relative| index_offset.checked_add(relative))
        .ok_or_else(|| ErofsError::Invalid("compression index address overflows".to_string()))?;
    let bytes = read_exact_at(source, offset, FULL_INDEX_BYTES as usize)?;
    let advise = read_u16(&bytes, 0, "compressed cluster advice")?;
    if advise & !(LCLUSTER_TYPE_MASK | LCLUSTER_HOLE) != 0 {
        return Err(ErofsError::Unsupported(format!(
            "compressed cluster advice flags {:#x}",
            advise & !(LCLUSTER_TYPE_MASK | LCLUSTER_HOLE)
        )));
    }
    if read_u16(&bytes, 2, "compressed cluster offset")? != 0 {
        return Err(ErofsError::Unsupported(
            "non-zero compressed cluster offsets".to_string(),
        ));
    }
    if advise & LCLUSTER_HOLE != 0 {
        return Ok(ClusterKind::Hole);
    }
    cluster_kind(
        advise & LCLUSTER_TYPE_MASK,
        u64::from(read_u32(&bytes, 4, "compressed cluster block")?),
        false,
    )
}

fn read_compact_cluster_kind(
    source: &SharedReader,
    layout: &CompactIndexLayout,
    index: u64,
) -> Result<ClusterKind> {
    let location = layout.locate(index)?;
    let bytes = read_exact_at(source, location.offset, location.bytes)?;
    for prior in 0..=location.entry_index {
        let (kind, cluster_offset) =
            decode_compact_entry(&bytes, prior, location.encoded_bits, location.entries)?;
        if kind == LCLUSTER_TYPE_NONHEAD {
            return Err(ErofsError::Unsupported(
                "multi-cluster compact compressed extents".to_string(),
            ));
        }
        if prior == location.entry_index {
            if cluster_offset != 0 {
                return Err(ErofsError::Unsupported(
                    "non-zero compact compressed cluster offsets".to_string(),
                ));
            }
            let base = u64::from(read_u32(
                &bytes,
                location.bytes - 4,
                "compact compression block base",
            )?);
            let block = base.checked_add(prior as u64 + 1).ok_or_else(|| {
                ErofsError::Invalid("compact compression block address overflows".to_string())
            })?;
            return cluster_kind(kind, block, true);
        }
    }
    Err(ErofsError::Invalid(
        "compact compressed index has no target entry".to_string(),
    ))
}

fn cluster_kind(kind: u16, block: u64, compact: bool) -> Result<ClusterKind> {
    match kind {
        LCLUSTER_TYPE_PLAIN => Ok(ClusterKind::Plain(block)),
        LCLUSTER_TYPE_HEAD1 => Ok(ClusterKind::Lz4(block)),
        LCLUSTER_TYPE_NONHEAD => Err(ErofsError::Unsupported(format!(
            "multi-cluster {}compressed extents",
            if compact { "compact " } else { "" }
        ))),
        LCLUSTER_TYPE_HEAD2 => Err(ErofsError::Unsupported(format!(
            "secondary {}compression algorithms",
            if compact { "compact " } else { "" }
        ))),
        _ => Err(ErofsError::Invalid(
            "unreachable compressed cluster type".to_string(),
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

fn validate_full_index_table(
    index_offset: u64,
    size: u64,
    block_size: usize,
    volume_offset: u64,
    block_count: u64,
) -> Result<()> {
    let index_bytes = size
        .div_ceil(block_size as u64)
        .checked_mul(FULL_INDEX_BYTES)
        .ok_or_else(|| ErofsError::Invalid("compression index table overflows".to_string()))?;
    validate_metadata_end(
        index_offset,
        index_bytes,
        block_size,
        volume_offset,
        block_count,
    )
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

fn packed_bytes(entries: u64, entries_per_pack: u64, pack_bytes: u64) -> Result<u64> {
    entries
        .div_ceil(entries_per_pack)
        .checked_mul(pack_bytes)
        .ok_or_else(|| ErofsError::Invalid("compact index byte count overflows".to_string()))
}

fn compact_pack_location(
    segment_offset: u64,
    relative_index: u64,
    entries_per_pack: u64,
    pack_bytes: u64,
) -> Result<CompactPackLocation> {
    let pack = relative_index / entries_per_pack;
    let offset = pack
        .checked_mul(pack_bytes)
        .and_then(|bytes| segment_offset.checked_add(bytes))
        .ok_or_else(|| ErofsError::Invalid("compact pack offset overflows".to_string()))?;
    Ok(CompactPackLocation {
        offset,
        bytes: pack_bytes as usize,
        entries: entries_per_pack as usize,
        entry_index: (relative_index % entries_per_pack) as usize,
        encoded_bits: ((pack_bytes - 4) * 8 / entries_per_pack) as u32,
    })
}

fn decode_compact_entry(
    bytes: &[u8],
    index: usize,
    encoded_bits: u32,
    entries: usize,
) -> Result<(u16, u16)> {
    if index >= entries {
        return Err(ErofsError::Invalid(
            "compact entry index exceeds its pack".to_string(),
        ));
    }
    let bit_offset = (index as u32)
        .checked_mul(encoded_bits)
        .ok_or_else(|| ErofsError::Invalid("compact entry bit offset overflows".to_string()))?;
    let byte_offset = (bit_offset / 8) as usize;
    let value = read_u32(bytes, byte_offset, "compact compression entry")? >> (bit_offset % 8);
    let encoded_mask = (1u32 << encoded_bits) - 1;
    if value & encoded_mask & !((1u32 << COMPACT_VALUE_BITS) - 1) != 0 {
        return Err(ErofsError::Invalid(
            "compact compression entry reserved bits are non-zero".to_string(),
        ));
    }
    Ok((
        ((value >> COMPACT_LOW_BITS) & u32::from(LCLUSTER_TYPE_MASK)) as u16,
        (value & ((1u32 << COMPACT_LOW_BITS) - 1)) as u16,
    ))
}

fn align_up(value: u64, alignment: u64) -> Result<u64> {
    value
        .checked_add(alignment - 1)
        .map(|sum| sum / alignment * alignment)
        .ok_or_else(|| ErofsError::Invalid("compression map alignment overflows".to_string()))
}
