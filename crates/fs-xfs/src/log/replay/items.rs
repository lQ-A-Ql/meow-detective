//! Application of reassembled log items to volume-relative patches.
//!
//! Item handlers mirror the kernel's `commit_pass2` operations:
//!
//! - `XFS_LI_BUF` (`xlog_recover_buf_commit_pass2`): data regions map in
//!   order onto the set bits of the 128-byte-chunk dirty bitmap. The grouped
//!   buffer action is finalized later, after the on-disk LSN check and write
//!   verifier have run against the current volume bytes.
//! - `XFS_LI_INODE` (`xlog_recover_inode_commit_pass2`): the log dinode core
//!   is converted to on-disk form with the transaction LSN and a fresh CRC;
//!   data/attr fork regions land in the inode's literal area.
//! - `XFS_LI_ICREATE` (`xlog_recover_icreate_commit_pass2` +
//!   `xfs_ialloc_inode_init`): the item carries only a 28-byte descriptor
//!   whose fields after type/size are big-endian; recovery regenerates a
//!   zeroed inode cluster, exactly like the kernel.
//! - Buffer cancellation (`XFS_BLF_CANCEL`) is collected over the whole log
//!   before anything is applied, like the kernel's pass1/pass2 split.
//!
//! Everything else (EFI/EFD, quotas, intents, ...) is skipped and counted.

use super::super::{XfsLogError, XfsLogFormat, XFS_LI_BUF, XFS_LI_ICREATE, XFS_LI_INODE};
use super::assemble::{AssembledItem, CommittedTransaction};
use super::dinode;
use super::sink::{CancelTable, PatchSink};
use super::{ReplayGeometry, XfsBufferReplay, XfsInodeReplay, XfsReplayAction, XfsReplayPatch};

const XFS_BLF_CANCEL: u16 = 1 << 1;
const BLF_CHUNK_BYTES: usize = 128;
const MAX_DATA_MAP_WORDS: usize = 17;
const INODE_LOG_FORMAT_SIZE: usize = 56;
const ICREATE_LOG_SIZE: usize = 28;
const XFS_ILOG_CORE: u32 = 0x001;
const XFS_ILOG_DFORK: u32 = 0x002 | 0x004;
const XFS_ILOG_AFORK: u32 = 0x040 | 0x080;
const XFS_ILOG_BROOTS: u32 = 0x008 | 0x100;
const XFS_ILOG_DEV: u32 = 0x010;

pub(super) struct ApplyOutcome {
    pub(super) actions: Vec<XfsReplayAction>,
    pub(super) skipped_items: u32,
}

/// Apply every committed transaction in log order, buffer-class items
/// (ICREATE, then BUF) before inode items within each transaction, matching
/// the kernel's `xlog_recover_reorder_trans` bucket order.
pub(super) fn apply_transactions(
    geometry: &ReplayGeometry,
    transactions: &[CommittedTransaction],
) -> Result<ApplyOutcome, XfsLogError> {
    let mut cancels = CancelTable::default();
    for transaction in transactions {
        for item in &transaction.items {
            collect_cancel(transaction.format, item, &mut cancels);
        }
    }
    let mut sink = PatchSink::new(geometry.capacity()?);
    let mut skipped_items = 0u32;
    for transaction in transactions {
        let mut inode_items = Vec::new();
        for item in &transaction.items {
            match item_type(transaction.format, item) {
                Some(XFS_LI_ICREATE) => {
                    skipped_items += u32::from(!apply_icreate(
                        geometry,
                        &mut sink,
                        &cancels,
                        transaction.format,
                        item,
                    )?);
                }
                Some(XFS_LI_BUF) => {
                    skipped_items += u32::from(!apply_buffer(
                        &mut sink,
                        &mut cancels,
                        transaction.format,
                        item,
                        transaction.lsn,
                    )?);
                }
                Some(XFS_LI_INODE) => inode_items.push(item),
                _ => skipped_items = skipped_items.saturating_add(1),
            }
        }
        for item in inode_items {
            skipped_items += u32::from(!apply_inode(
                geometry,
                &mut sink,
                &cancels,
                transaction.format,
                item,
                transaction.lsn,
            )?);
        }
    }
    Ok(ApplyOutcome {
        actions: sink.actions,
        skipped_items,
    })
}

fn item_type(format: XfsLogFormat, item: &AssembledItem) -> Option<u16> {
    format.native_u16(item.regions.first()?, 0)
}

/// Pass 1: record the ranges of cancellation buffers so their stale
/// contents are never replayed into reallocated blocks.
fn collect_cancel(format: XfsLogFormat, item: &AssembledItem, cancels: &mut CancelTable) {
    if item_type(format, item) != Some(XFS_LI_BUF) {
        return;
    }
    let Some(descriptor) = parse_buf_descriptor(format, item) else {
        return;
    };
    if descriptor.flags & XFS_BLF_CANCEL != 0 {
        cancels.add(descriptor.blkno, descriptor.len);
    }
}

struct BufDescriptor {
    flags: u16,
    len: u32,
    blkno: u64,
    map: Vec<u32>,
}

/// `struct xfs_buf_log_format`: type/size host order, then blf_flags,
/// blf_len (basic blocks), blf_blkno (basic blocks) and the dirty bitmap of
/// host-order words over 128-byte chunks.
fn parse_buf_descriptor(format: XfsLogFormat, item: &AssembledItem) -> Option<BufDescriptor> {
    let region = item.regions.first()?;
    if region.len() < 20 {
        return None;
    }
    let flags = format.native_u16(region, 4)?;
    let len = u32::from(format.native_u16(region, 6)?);
    let blkno = format.native_i64(region, 8)?;
    let map_size = format.native_u32(region, 16)? as usize;
    if len == 0 || blkno < 0 || map_size > MAX_DATA_MAP_WORDS {
        return None;
    }
    if region.len() < 20 + map_size * 4 {
        return None;
    }
    let mut map = Vec::with_capacity(map_size);
    for word in 0..map_size {
        map.push(format.native_u32(region, 20 + word * 4)?);
    }
    Some(BufDescriptor {
        flags,
        len,
        blkno: blkno as u64,
        map,
    })
}

fn apply_buffer(
    sink: &mut PatchSink,
    cancels: &mut CancelTable,
    format: XfsLogFormat,
    item: &AssembledItem,
    lsn: u64,
) -> Result<bool, XfsLogError> {
    let Some(descriptor) = parse_buf_descriptor(format, item) else {
        return Ok(false);
    };
    if descriptor.flags & XFS_BLF_CANCEL != 0 {
        cancels.remove_one(descriptor.blkno, descriptor.len);
        return Ok(true);
    }
    if cancels.contains(descriptor.blkno, descriptor.len) {
        return Ok(true);
    }
    let Some(buffer_bytes) = u64::from(descriptor.len).checked_mul(512) else {
        return Ok(false);
    };
    let Some(base) = descriptor.blkno.checked_mul(512) else {
        return Ok(false);
    };
    if base
        .checked_add(buffer_bytes)
        .is_none_or(|end| end > sink.capacity)
    {
        return Ok(false);
    }
    let Ok(buffer_length) = usize::try_from(buffer_bytes) else {
        return Ok(false);
    };
    let total_bits = (buffer_length / BLF_CHUNK_BYTES).min(descriptor.map.len() * 32);
    let mut writes = Vec::new();
    let mut region_index = 1;
    let mut bit = 0;
    while let Some(set) = next_set_bit(&descriptor.map, bit, total_bits) {
        let contiguous = contiguous_set_bits(&descriptor.map, set, total_bits);
        let Some(region) = item.regions.get(region_index) else {
            return Ok(false);
        };
        if region.len() % BLF_CHUNK_BYTES != 0 {
            return Ok(false);
        }
        let chunks = contiguous.min(region.len() / BLF_CHUNK_BYTES);
        let offset = base + (set * BLF_CHUNK_BYTES) as u64;
        writes.push(XfsReplayPatch {
            offset,
            bytes: region[..chunks * BLF_CHUNK_BYTES].to_vec(),
        });
        region_index += 1;
        bit = set + chunks;
    }
    if writes.is_empty() || region_index != item.regions.len() {
        return Ok(false);
    }
    sink.push_buffer(XfsBufferReplay {
        offset: base,
        length: buffer_length,
        lsn,
        buffer_type: (descriptor.flags >> 11) & 0x1f,
        writes,
    })?;
    Ok(true)
}

fn next_set_bit(map: &[u32], from: usize, limit: usize) -> Option<usize> {
    (from..limit).find(|bit| map[bit / 32] >> (bit % 32) & 1 == 1)
}

fn contiguous_set_bits(map: &[u32], start: usize, limit: usize) -> usize {
    (start..limit)
        .take_while(|bit| map[bit / 32] >> (bit % 32) & 1 == 1)
        .count()
}

/// `struct xfs_inode_log_format` (64-bit variant): the descriptor fixes the
/// inode buffer (512-byte sectors) and byte offset the core lands at.
fn apply_inode(
    geometry: &ReplayGeometry,
    sink: &mut PatchSink,
    cancels: &CancelTable,
    format: XfsLogFormat,
    item: &AssembledItem,
    lsn: u64,
) -> Result<bool, XfsLogError> {
    let Some(descriptor) = item.regions.first() else {
        return Ok(false);
    };
    if descriptor.len() != INODE_LOG_FORMAT_SIZE {
        return Ok(false);
    }
    let Some(fields) = format.native_u32(descriptor, 4) else {
        return Ok(false);
    };
    if fields & XFS_ILOG_CORE == 0 || fields & XFS_ILOG_BROOTS != 0 {
        return Ok(false);
    }
    let (Some(inode_number), Some(blkno), Some(len), Some(boffset)) = (
        format.native_u64(descriptor, 16),
        format.native_i64(descriptor, 40),
        format.native_u32(descriptor, 48),
        format.native_u32(descriptor, 52),
    ) else {
        return Ok(false);
    };
    if blkno < 0 || len == 0 {
        return Ok(false);
    }
    if cancels.contains(blkno as u64, len) {
        return Ok(true);
    }
    let inode_size = u64::from(geometry.inode_size);
    let Some(base) = (blkno as u64)
        .checked_mul(512)
        .and_then(|start| start.checked_add(u64::from(boffset)))
    else {
        return Ok(false);
    };
    if u64::from(boffset) + inode_size > u64::from(len) * 512 {
        return Ok(false);
    }
    let Some(core) = item.regions.get(1) else {
        return Ok(false);
    };
    let Ok(disk_core) = dinode::log_core_to_disk(format, core, lsn) else {
        return Ok(false);
    };
    let mut writes = vec![(base, disk_core.to_vec())];
    if fields & XFS_ILOG_DEV != 0 {
        let Some(rdev) = format.native_u32(descriptor, 24) else {
            return Ok(false);
        };
        writes.push((
            base + dinode::V3_CORE_SIZE as u64,
            rdev.to_be_bytes().to_vec(),
        ));
    }
    if !plan_fork_writes(
        usize::from(geometry.inode_size),
        &disk_core,
        fields,
        item,
        base,
        &mut writes,
    ) {
        return Ok(false);
    }
    let writes = writes
        .into_iter()
        .map(|(offset, bytes)| XfsReplayPatch { offset, bytes })
        .collect();
    sink.push_inode(XfsInodeReplay {
        offset: base,
        length: usize::from(geometry.inode_size),
        lsn,
        inode_number,
        writes,
    })?;
    Ok(true)
}

/// Plan the data-fork and attr-fork writes of an inode item into the
/// literal area (starting at the v3 core size), bounded by the inode size.
fn plan_fork_writes(
    inode_size: usize,
    disk_core: &[u8],
    fields: u32,
    item: &AssembledItem,
    base: u64,
    writes: &mut Vec<(u64, Vec<u8>)>,
) -> bool {
    let literal = dinode::V3_CORE_SIZE;
    let forkoff = usize::from(disk_core[82]) * 8;
    if fields & XFS_ILOG_DFORK != 0 {
        let Some(region) = item.regions.get(2) else {
            return false;
        };
        let fits =
            literal + region.len() <= inode_size && (forkoff == 0 || region.len() <= forkoff);
        if !fits {
            return false;
        }
        writes.push((base + literal as u64, region.clone()));
    }
    if fields & XFS_ILOG_AFORK != 0 {
        let index = if fields & XFS_ILOG_DFORK != 0 { 3 } else { 2 };
        let Some(region) = item.regions.get(index) else {
            return false;
        };
        if forkoff == 0 || literal + forkoff + region.len() > inode_size {
            return false;
        }
        writes.push((base + (literal + forkoff) as u64, region.clone()));
    }
    true
}

/// `struct xfs_icreate_log`: type/size host order, every later field
/// big-endian. The item logs no inode images; recovery regenerates the
/// freshly allocated cluster exactly like `xfs_ialloc_inode_init`.
fn apply_icreate(
    geometry: &ReplayGeometry,
    sink: &mut PatchSink,
    cancels: &CancelTable,
    format: XfsLogFormat,
    item: &AssembledItem,
) -> Result<bool, XfsLogError> {
    let Some(descriptor) = item.regions.first() else {
        return Ok(false);
    };
    if item.regions.len() != 1
        || descriptor.len() < ICREATE_LOG_SIZE
        || format.native_u16(descriptor, 2) != Some(1)
    {
        return Ok(false);
    }
    let (Some(ag), Some(agbno), Some(count), Some(isize), Some(length), Some(generation)) = (
        be32_at(descriptor, 4),
        be32_at(descriptor, 8),
        be32_at(descriptor, 12),
        be32_at(descriptor, 16),
        be32_at(descriptor, 20),
        be32_at(descriptor, 24),
    ) else {
        return Ok(false);
    };
    let inopblog = u32::from(geometry.inopblog);
    let agshift = u32::from(geometry.agblklog) + inopblog;
    if inopblog >= 32 || agshift >= 64 {
        return Ok(false);
    }
    let valid = ag < geometry.ag_count
        && agbno > 0
        && u64::from(agbno) < geometry.ag_blocks
        && isize == u32::from(geometry.inode_size)
        && count > 0
        && length > 0
        && u64::from(length) < geometry.ag_blocks
        && count >> inopblog == length
        && count == length << inopblog;
    if !valid {
        return Ok(false);
    }
    let Some(start) =
        (u64::from(ag) * geometry.ag_blocks + u64::from(agbno)).checked_mul(geometry.block_size)
    else {
        return Ok(false);
    };
    let sectors_per_block = geometry.block_size / 512;
    if cancels.overlaps(start / 512, u64::from(length) * sectors_per_block) {
        return Ok(true);
    }
    let cluster_len = usize::try_from(u64::from(length) * geometry.block_size)
        .map_err(|_| XfsLogError::InvalidData("icreate cluster length overflows".into()))?;
    let mut cluster = vec![0u8; cluster_len];
    let ino_base = (u64::from(ag) << agshift) | (u64::from(agbno) << inopblog);
    for index in 0..count as usize {
        let offset = index * isize as usize;
        let Some(slot) = cluster.get_mut(offset..offset + isize as usize) else {
            return Ok(false);
        };
        let inode = dinode::fresh_v3_inode(
            isize as usize,
            generation,
            ino_base + index as u64,
            &geometry.metadata_uuid,
        );
        slot.copy_from_slice(&inode);
    }
    sink.push(start, cluster)
}

/// ICREATE scalar fields are `__be32` on the wire regardless of the host
/// byte order (cpu_to_be32 at the write site since the item was introduced).
fn be32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}
