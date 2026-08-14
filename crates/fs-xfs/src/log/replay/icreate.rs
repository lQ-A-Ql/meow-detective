//! Recovery of inode-allocation intent items.

use super::super::{XfsLogError, XfsLogFormat};
use super::assemble::AssembledItem;
use super::dinode;
use super::sink::{CancelTable, PatchSink};
use super::{ReplayDisposition, ReplayGeometry};

const ICREATE_LOG_SIZE: usize = 28;

/// `struct xfs_icreate_log`: type/size host order, every later field
/// big-endian. The item logs no inode images; recovery regenerates the
/// freshly allocated cluster exactly like `xfs_ialloc_inode_init`.
pub(super) fn apply(
    geometry: &ReplayGeometry,
    sink: &mut PatchSink,
    cancels: &CancelTable,
    format: XfsLogFormat,
    item: &AssembledItem,
) -> Result<ReplayDisposition, XfsLogError> {
    let descriptor = item
        .regions
        .first()
        .ok_or_else(|| XfsLogError::InvalidData("ICREATE descriptor is missing".to_string()))?;
    if item.regions.len() != 1
        || descriptor.len() < ICREATE_LOG_SIZE
        || format.native_u16(descriptor, 2) != Some(1)
    {
        return Err(XfsLogError::InvalidData(
            "ICREATE descriptor has an invalid size or region count".to_string(),
        ));
    }
    let (Some(ag), Some(agbno), Some(count), Some(isize), Some(length), Some(generation)) = (
        be32_at(descriptor, 4),
        be32_at(descriptor, 8),
        be32_at(descriptor, 12),
        be32_at(descriptor, 16),
        be32_at(descriptor, 20),
        be32_at(descriptor, 24),
    ) else {
        return Err(XfsLogError::InvalidData(
            "ICREATE descriptor fields are truncated".to_string(),
        ));
    };
    let inopblog = u32::from(geometry.inopblog);
    let agshift = u32::from(geometry.agblklog) + inopblog;
    if inopblog >= 32 || agshift >= 64 {
        return Err(XfsLogError::InvalidGeometry(
            "ICREATE inode addressing shift is invalid".to_string(),
        ));
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
        return unsafe_replay("ICREATE geometry is inconsistent with the filesystem");
    }
    let Some(start) =
        (u64::from(ag) * geometry.ag_blocks + u64::from(agbno)).checked_mul(geometry.block_size)
    else {
        return Err(XfsLogError::InvalidData(
            "ICREATE write offset overflows".to_string(),
        ));
    };
    let sectors_per_block = geometry.block_size / 512;
    if cancels.overlaps(start / 512, u64::from(length) * sectors_per_block) {
        return Ok(ReplayDisposition::Cancelled);
    }
    let cluster_len = usize::try_from(u64::from(length) * geometry.block_size)
        .map_err(|_| XfsLogError::InvalidData("icreate cluster length overflows".into()))?;
    let mut cluster = vec![0u8; cluster_len];
    let ino_base = (u64::from(ag) << agshift) | (u64::from(agbno) << inopblog);
    for index in 0..count as usize {
        let offset = index * isize as usize;
        let Some(slot) = cluster.get_mut(offset..offset + isize as usize) else {
            return Err(XfsLogError::InvalidData(
                "ICREATE inode cluster is internally inconsistent".to_string(),
            ));
        };
        let inode = dinode::fresh_v3_inode(
            isize as usize,
            generation,
            ino_base + index as u64,
            &geometry.metadata_uuid,
        );
        slot.copy_from_slice(&inode);
    }
    if !sink.push(start, cluster)? {
        return unsafe_replay("ICREATE write lies outside filesystem geometry");
    }
    Ok(ReplayDisposition::Applied)
}

fn unsafe_replay<T>(message: impl Into<String>) -> Result<T, XfsLogError> {
    Err(XfsLogError::UnsafeReplay(message.into()))
}

/// ICREATE scalar fields are `__be32` on the wire regardless of the host
/// byte order (cpu_to_be32 at the write site since the item was introduced).
fn be32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}
