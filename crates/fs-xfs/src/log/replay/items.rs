//! Application of reassembled log items to volume-relative patches.
//!
//! BUF, INODE, and ICREATE handlers mirror their kernel `commit_pass2`
//! operations. Buffer cancellation is collected over the complete log before
//! actions are emitted, preserving the kernel's pass1/pass2 ordering.
//!
//! EFI/EFD pairs are tracked because a pending EFI would cause the kernel to
//! free extents after log recovery. QUOTAOFF is a validated pass-2 no-op;
//! every other unsupported item aborts planning.

use std::collections::HashSet;

use super::super::{
    XfsLogError, XfsLogFormat, XFS_LI_BUF, XFS_LI_DQUOT, XFS_LI_EFD, XFS_LI_EFI, XFS_LI_ICREATE,
    XFS_LI_INODE, XFS_LI_QUOTAOFF,
};
use super::assemble::{AssembledItem, CommittedTransaction};
use super::deferred;
use super::dinode;
use super::icreate;
use super::sink::{CancelTable, PatchSink};
use super::{
    ReplayDisposition, ReplayGeometry, XfsBufferReplay, XfsInodeReplay, XfsReplayAction,
    XfsReplayPatch,
};

const XFS_BLF_CANCEL: u16 = 1 << 1;
const XFS_BLF_INODE_BUF: u16 = 1;
const BLF_CHUNK_BYTES: usize = 128;
const MAX_DATA_MAP_WORDS: usize = 17;
const INODE_LOG_FORMAT_SIZE: usize = 56;
const XFS_ILOG_CORE: u32 = 0x001;
const XFS_ILOG_DFORK: u32 = 0x002 | 0x004;
const XFS_ILOG_AFORK: u32 = 0x040 | 0x080;
const XFS_ILOG_BROOTS: u32 = 0x008 | 0x100;
const XFS_ILOG_DEV: u32 = 0x010;
const XFS_ILOG_OWNERS: u32 = 0x200 | 0x400;

pub(super) struct ApplyOutcome {
    pub(super) actions: Vec<XfsReplayAction>,
}
/// Apply every committed transaction in log order: ordinary buffer-class
/// items first, inode items second, and inode-unlink buffers last. The final
/// bucket preserves the kernel's `xlog_recover_reorder_trans` rule that an
/// inode core must be restored before its `di_next_unlinked` field.
pub(super) fn apply_transactions(
    geometry: &ReplayGeometry,
    transactions: &[CommittedTransaction],
) -> Result<ApplyOutcome, XfsLogError> {
    let mut cancels = CancelTable::default();
    for transaction in transactions {
        for item in &transaction.items {
            collect_cancel(transaction.format, item, &mut cancels)?;
        }
    }
    let mut sink = PatchSink::new(geometry.capacity()?);
    let mut pending_efi = HashSet::new();
    for transaction in transactions {
        let mut inode_items = Vec::new();
        let mut inode_unlinked_buffers = Vec::new();
        for item in &transaction.items {
            match item_type(transaction.format, item) {
                Some(XFS_LI_ICREATE) => {
                    let _disposition =
                        icreate::apply(geometry, &mut sink, &cancels, transaction.format, item)?;
                }
                Some(XFS_LI_BUF) => {
                    if is_inode_unlinked_buffer(transaction.format, item)? {
                        inode_unlinked_buffers.push(item);
                    } else {
                        let _disposition = apply_buffer(
                            geometry,
                            &mut sink,
                            &mut cancels,
                            transaction.format,
                            item,
                            transaction.lsn,
                        )?;
                    }
                }
                Some(XFS_LI_INODE) => inode_items.push(item),
                Some(XFS_LI_EFI) => {
                    let id = deferred::parse_id(transaction.format, item, "EFI")?;
                    if !pending_efi.insert(id) {
                        return unsafe_replay(format!("duplicate pending EFI id {id:#x}"));
                    }
                }
                Some(XFS_LI_EFD) => {
                    let id = deferred::parse_id(transaction.format, item, "EFD")?;
                    pending_efi.remove(&id);
                    let _disposition = ReplayDisposition::DeferredResolved;
                }
                Some(XFS_LI_QUOTAOFF) => deferred::validate_quotaoff(transaction.format, item)?,
                Some(XFS_LI_DQUOT) => {
                    return unsafe_replay("DQUOT replay is not implemented");
                }
                Some(kind) => {
                    return unsafe_replay(format!("unsupported log item type {kind:#06x}"));
                }
                None => return unsafe_replay("log item type is missing or malformed"),
            }
        }
        for item in inode_items {
            let _disposition = apply_inode(
                geometry,
                &mut sink,
                &cancels,
                transaction.format,
                item,
                transaction.lsn,
            )?;
        }
        for item in inode_unlinked_buffers {
            let _disposition = apply_buffer(
                geometry,
                &mut sink,
                &mut cancels,
                transaction.format,
                item,
                transaction.lsn,
            )?;
        }
    }
    if !pending_efi.is_empty() {
        return unsafe_replay(format!(
            "{} EFI intent(s) remain pending and require extent-free replay",
            pending_efi.len()
        ));
    }
    Ok(ApplyOutcome {
        actions: sink.actions,
    })
}

fn is_inode_unlinked_buffer(
    format: XfsLogFormat,
    item: &AssembledItem,
) -> Result<bool, XfsLogError> {
    let descriptor = parse_buf_descriptor(format, item)
        .ok_or_else(|| XfsLogError::InvalidData("malformed BUF descriptor".to_string()))?;
    Ok(descriptor.flags & XFS_BLF_INODE_BUF != 0)
}

fn item_type(format: XfsLogFormat, item: &AssembledItem) -> Option<u16> {
    format.native_u16(item.regions.first()?, 0)
}

/// Pass 1: record the ranges of cancellation buffers so their stale
/// contents are never replayed into reallocated blocks.
fn collect_cancel(
    format: XfsLogFormat,
    item: &AssembledItem,
    cancels: &mut CancelTable,
) -> Result<(), XfsLogError> {
    if item_type(format, item) != Some(XFS_LI_BUF) {
        return Ok(());
    }
    let descriptor = parse_buf_descriptor(format, item)
        .ok_or_else(|| XfsLogError::InvalidData("malformed BUF descriptor".to_string()))?;
    if descriptor.flags & XFS_BLF_CANCEL != 0 {
        cancels.add(descriptor.blkno, descriptor.len);
    }
    Ok(())
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
    geometry: &ReplayGeometry,
    sink: &mut PatchSink,
    cancels: &mut CancelTable,
    format: XfsLogFormat,
    item: &AssembledItem,
    lsn: u64,
) -> Result<ReplayDisposition, XfsLogError> {
    let descriptor = parse_buf_descriptor(format, item)
        .ok_or_else(|| XfsLogError::InvalidData("malformed BUF descriptor".to_string()))?;
    if descriptor.flags & XFS_BLF_CANCEL != 0 {
        cancels.remove_one(descriptor.blkno, descriptor.len);
        return Ok(ReplayDisposition::Cancelled);
    }
    if cancels.contains(descriptor.blkno, descriptor.len) {
        return Ok(ReplayDisposition::Cancelled);
    }
    let buffer_bytes = u64::from(descriptor.len)
        .checked_mul(512)
        .ok_or_else(|| XfsLogError::InvalidData("BUF length overflows".to_string()))?;
    let base = descriptor
        .blkno
        .checked_mul(512)
        .ok_or_else(|| XfsLogError::InvalidData("BUF offset overflows".to_string()))?;
    if base
        .checked_add(buffer_bytes)
        .is_none_or(|end| end > sink.capacity)
    {
        return unsafe_replay("BUF write lies outside filesystem geometry");
    }
    let buffer_length = usize::try_from(buffer_bytes)
        .map_err(|_| XfsLogError::InvalidData("BUF length exceeds host limits".to_string()))?;
    let total_bits = (buffer_length / BLF_CHUNK_BYTES).min(descriptor.map.len() * 32);
    if descriptor.map.iter().enumerate().any(|(word, value)| {
        (0..32).any(|bit| word * 32 + bit >= total_bits && value >> bit & 1 != 0)
    }) {
        return unsafe_replay("BUF dirty bitmap addresses data beyond the buffer");
    }
    let mut writes = Vec::new();
    let mut region_index = 1;
    let mut bit = 0;
    while let Some(set) = next_set_bit(&descriptor.map, bit, total_bits) {
        let contiguous = contiguous_set_bits(&descriptor.map, set, total_bits);
        let region = item.regions.get(region_index).ok_or_else(|| {
            XfsLogError::InvalidData("BUF dirty bitmap has no matching data region".to_string())
        })?;
        if region.is_empty() || region.len() % BLF_CHUNK_BYTES != 0 {
            return Err(XfsLogError::InvalidData(
                "BUF data region is not 128-byte aligned".to_string(),
            ));
        }
        let chunks = region.len() / BLF_CHUNK_BYTES;
        if chunks > contiguous {
            return unsafe_replay("BUF data region exceeds its dirty bitmap run");
        }
        let offset = base + (set * BLF_CHUNK_BYTES) as u64;
        writes.push(XfsReplayPatch {
            offset,
            bytes: region[..chunks * BLF_CHUNK_BYTES].to_vec(),
        });
        region_index += 1;
        bit = set + chunks;
    }
    if writes.is_empty() || region_index != item.regions.len() {
        return unsafe_replay("BUF regions do not exactly match the dirty bitmap");
    }
    sink.push_buffer(XfsBufferReplay {
        offset: base,
        length: buffer_length,
        lsn,
        buffer_type: (descriptor.flags >> 11) & 0x1f,
        inode_unlinked_only: descriptor.flags & XFS_BLF_INODE_BUF != 0,
        inode_size: geometry.inode_size,
        ag_inode_count: geometry.ag_blocks << geometry.inopblog,
        writes,
    })?;
    Ok(ReplayDisposition::Applied)
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
) -> Result<ReplayDisposition, XfsLogError> {
    let descriptor = item
        .regions
        .first()
        .ok_or_else(|| XfsLogError::InvalidData("INODE descriptor is missing".to_string()))?;
    if descriptor.len() != INODE_LOG_FORMAT_SIZE {
        return Err(XfsLogError::InvalidData(format!(
            "INODE descriptor length {} is not {INODE_LOG_FORMAT_SIZE}",
            descriptor.len()
        )));
    }
    let fields = format
        .native_u32(descriptor, 4)
        .ok_or_else(|| XfsLogError::InvalidData("INODE fields are malformed".to_string()))?;
    if fields & XFS_ILOG_CORE == 0 {
        return unsafe_replay("INODE item does not contain its core");
    }
    if fields & XFS_ILOG_BROOTS != 0 {
        return unsafe_replay("INODE btree-root conversion is not implemented");
    }
    if fields & XFS_ILOG_OWNERS != 0 {
        return unsafe_replay("INODE btree owner rewrite is not implemented");
    }
    let (Some(inode_number), Some(blkno), Some(len), Some(boffset)) = (
        format.native_u64(descriptor, 16),
        format.native_i64(descriptor, 40),
        format.native_u32(descriptor, 48),
        format.native_u32(descriptor, 52),
    ) else {
        return Err(XfsLogError::InvalidData(
            "INODE descriptor fields are truncated".to_string(),
        ));
    };
    if blkno < 0 || len == 0 {
        return Err(XfsLogError::InvalidData(
            "INODE descriptor has an invalid buffer range".to_string(),
        ));
    }
    if cancels.contains(blkno as u64, len) {
        return Ok(ReplayDisposition::Cancelled);
    }
    let inode_size = u64::from(geometry.inode_size);
    let Some(base) = (blkno as u64)
        .checked_mul(512)
        .and_then(|start| start.checked_add(u64::from(boffset)))
    else {
        return Err(XfsLogError::InvalidData(
            "INODE replay offset overflows".to_string(),
        ));
    };
    if u64::from(boffset) + inode_size > u64::from(len) * 512 {
        return unsafe_replay("INODE write lies outside its logged buffer");
    }
    let core = item
        .regions
        .get(1)
        .ok_or_else(|| XfsLogError::InvalidData("INODE core region is missing".to_string()))?;
    let disk_core = dinode::log_core_to_disk(format, core, lsn)
        .map_err(|error| XfsLogError::InvalidData(format!("invalid INODE core: {error}")))?;
    let mut writes = vec![(base, disk_core.to_vec())];
    if fields & XFS_ILOG_DEV != 0 {
        let rdev = format.native_u32(descriptor, 24).ok_or_else(|| {
            XfsLogError::InvalidData("INODE device field is truncated".to_string())
        })?;
        writes.push((
            base + dinode::V3_CORE_SIZE as u64,
            rdev.to_be_bytes().to_vec(),
        ));
    }
    plan_fork_writes(
        usize::from(geometry.inode_size),
        &disk_core,
        fields,
        item,
        base,
        &mut writes,
    )?;
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
    Ok(ReplayDisposition::Applied)
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
) -> Result<(), XfsLogError> {
    let literal = dinode::V3_CORE_SIZE;
    let forkoff = usize::from(disk_core[82]) * 8;
    if fields & XFS_ILOG_DFORK != 0 {
        let region = item.regions.get(2).ok_or_else(|| {
            XfsLogError::InvalidData("INODE data-fork region is missing".to_string())
        })?;
        let fits =
            literal + region.len() <= inode_size && (forkoff == 0 || region.len() <= forkoff);
        if !fits {
            return unsafe_replay("INODE data fork exceeds the literal area");
        }
        writes.push((base + literal as u64, region.clone()));
    }
    if fields & XFS_ILOG_AFORK != 0 {
        let index = if fields & XFS_ILOG_DFORK != 0 { 3 } else { 2 };
        let region = item.regions.get(index).ok_or_else(|| {
            XfsLogError::InvalidData("INODE attr-fork region is missing".to_string())
        })?;
        if forkoff == 0 || literal + forkoff + region.len() > inode_size {
            return unsafe_replay("INODE attr fork exceeds the literal area");
        }
        writes.push((base + (literal + forkoff) as u64, region.clone()));
    }
    Ok(())
}

fn unsafe_replay<T>(message: impl Into<String>) -> Result<T, XfsLogError> {
    Err(XfsLogError::UnsafeReplay(message.into()))
}
