//! Recovery-time metadata LSN checks and v5 buffer write verifiers.
//!
//! RHEL7 recovery first reads the current buffer and skips an older logged
//! image (`xlog_recover_get_buf_lsn`). When it does replay a CRC-enabled
//! metadata buffer, the selected `b_ops` write verifier stamps the recovery
//! LSN and recomputes CRC32C. Keeping both operations here prevents a valid
//! source buffer from becoming stale merely because a partial log region was
//! copied over it.

use super::super::checksum::crc32c;

const XFS_SB_MAGIC: u32 = 0x5846_5342;
const XFS_AGF_MAGIC: u32 = 0x5841_4746;
const XFS_AGFL_MAGIC: u32 = 0x5841_464c;
const XFS_AGI_MAGIC: u32 = 0x5841_4749;
const XFS_ABTB_CRC_MAGIC: u32 = 0x4142_3342;
const XFS_ABTC_CRC_MAGIC: u32 = 0x4142_3343;
const XFS_IBT_CRC_MAGIC: u32 = 0x4941_4233;
const XFS_FIBT_CRC_MAGIC: u32 = 0x4649_4233;
const XFS_BMAP_CRC_MAGIC: u32 = 0x424d_4133;
const XFS_SYMLINK_MAGIC: u32 = 0x5853_4c4d;
const XFS_DIR3_BLOCK_MAGIC: u32 = 0x5844_4233;
const XFS_DIR3_DATA_MAGIC: u32 = 0x5844_4433;
const XFS_DIR3_FREE_MAGIC: u32 = 0x5844_4633;
const XFS_ATTR3_RMT_MAGIC: u32 = 0x5841_524d;
const XFS_DIR3_LEAF1_MAGIC: u16 = 0x3df1;
const XFS_DIR3_LEAFN_MAGIC: u16 = 0x3dff;
const XFS_DA3_NODE_MAGIC: u16 = 0x3ebe;
const XFS_ATTR3_LEAF_MAGIC: u16 = 0x3bee;

const BLFT_BTREE: u16 = 4;
const BLFT_AGF: u16 = 5;
const BLFT_AGFL: u16 = 6;
const BLFT_AGI: u16 = 7;
const BLFT_SYMLINK: u16 = 9;
const BLFT_DIR_BLOCK: u16 = 10;
const BLFT_DIR_DATA: u16 = 11;
const BLFT_DIR_FREE: u16 = 12;
const BLFT_DIR_LEAF1: u16 = 13;
const BLFT_DIR_LEAFN: u16 = 14;
const BLFT_DA_NODE: u16 = 15;
const BLFT_ATTR_LEAF: u16 = 16;
const BLFT_ATTR_RMT: u16 = 17;
const BLFT_SB: u16 = 18;

#[derive(Clone, Copy)]
struct MetadataLayout {
    crc: usize,
    lsn: usize,
    uuid: usize,
}

pub(super) fn current_lsn(bytes: &[u8], fs_uuid: &[u8; 16]) -> Option<u64> {
    let layout = layout_from_magic(bytes, fs_uuid)?;
    if bytes.get(layout.uuid..layout.uuid + 16)? != fs_uuid {
        return None;
    }
    be_u64(bytes, layout.lsn)
}

/// Kernel `XFS_LSN_CMP`: cycles and block numbers are compared as separate
/// fields, which is equivalent to the packed ordering except at wrap points
/// where a plain integer comparison would mis-order a recovered record.
pub(super) fn lsn_is_at_or_after(current: u64, replay: u64) -> bool {
    let current_cycle = current >> 32;
    let replay_cycle = replay >> 32;
    current_cycle > replay_cycle
        || (current_cycle == replay_cycle && (current as u32) >= (replay as u32))
}

pub(super) fn requires_verifier(buffer_type: u16) -> bool {
    matches!(
        buffer_type,
        BLFT_BTREE
            | BLFT_AGF
            | BLFT_AGFL
            | BLFT_AGI
            | BLFT_SYMLINK
            | BLFT_DIR_BLOCK
            | BLFT_DIR_DATA
            | BLFT_DIR_FREE
            | BLFT_DIR_LEAF1
            | BLFT_DIR_LEAFN
            | BLFT_DA_NODE
            | BLFT_ATTR_LEAF
            | BLFT_ATTR_RMT
            | BLFT_SB
    )
}

/// Stamp the write-verifier fields for the type encoded in `blf_flags`.
/// Returns a reason when post-replay magic, UUID, or field bounds do not
/// match the verifier selected by the logged buffer type.
pub(super) fn seal(
    bytes: &mut [u8],
    buffer_type: u16,
    lsn: u64,
    fs_uuid: &[u8; 16],
) -> Result<(), String> {
    let layout = layout_for_type(bytes, buffer_type, fs_uuid)
        .ok_or_else(|| "metadata magic does not match the logged buffer type".to_string())?;
    let actual_uuid = bytes
        .get(layout.uuid..layout.uuid + 16)
        .ok_or_else(|| "metadata UUID lies outside the buffer".to_string())?;
    if actual_uuid != fs_uuid {
        return Err(format!(
            "metadata UUID {} does not match filesystem {}",
            hex_uuid(actual_uuid),
            hex_uuid(fs_uuid)
        ));
    }
    if bytes.get(layout.crc..layout.crc + 4).is_none()
        || bytes.get(layout.lsn..layout.lsn + 8).is_none()
    {
        return Err("metadata verifier fields lie outside the buffer".to_string());
    }
    bytes[layout.lsn..layout.lsn + 8].copy_from_slice(&lsn.to_be_bytes());
    stamp_crc32c(bytes, layout.crc);
    Ok(())
}

fn hex_uuid(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn layout_for_type(bytes: &[u8], buffer_type: u16, fs_uuid: &[u8; 16]) -> Option<MetadataLayout> {
    let magic = be_u32(bytes, 0)?;
    match buffer_type {
        BLFT_BTREE => btree_layout(magic),
        BLFT_AGF if magic == XFS_AGF_MAGIC => Some(layout(216, 208, 64)),
        BLFT_AGFL if magic == XFS_AGFL_MAGIC => Some(layout(32, 24, 8)),
        BLFT_AGI if magic == XFS_AGI_MAGIC => Some(layout(312, 320, 296)),
        BLFT_SYMLINK if magic == XFS_SYMLINK_MAGIC => Some(layout(12, 48, 16)),
        BLFT_DIR_BLOCK | BLFT_DIR_DATA | BLFT_DIR_FREE
            if matches!(
                magic,
                XFS_DIR3_BLOCK_MAGIC | XFS_DIR3_DATA_MAGIC | XFS_DIR3_FREE_MAGIC
            ) =>
        {
            Some(layout(4, 16, 24))
        }
        BLFT_DIR_LEAF1 | BLFT_DIR_LEAFN | BLFT_DA_NODE | BLFT_ATTR_LEAF => {
            da_layout(bytes, buffer_type)
        }
        BLFT_ATTR_RMT if magic == XFS_ATTR3_RMT_MAGIC => Some(layout(12, 48, 16)),
        BLFT_SB if magic == XFS_SB_MAGIC => superblock_layout(bytes, fs_uuid),
        _ => None,
    }
}

fn layout_from_magic(bytes: &[u8], fs_uuid: &[u8; 16]) -> Option<MetadataLayout> {
    let magic = be_u32(bytes, 0)?;
    match magic {
        XFS_AGF_MAGIC => Some(layout(216, 208, 64)),
        XFS_AGFL_MAGIC => Some(layout(32, 24, 8)),
        XFS_AGI_MAGIC => Some(layout(312, 320, 296)),
        XFS_SYMLINK_MAGIC | XFS_ATTR3_RMT_MAGIC => Some(layout(12, 48, 16)),
        XFS_DIR3_BLOCK_MAGIC | XFS_DIR3_DATA_MAGIC | XFS_DIR3_FREE_MAGIC => Some(layout(4, 16, 24)),
        XFS_SB_MAGIC => superblock_layout(bytes, fs_uuid),
        _ => btree_layout(magic).or_else(|| da_layout_from_magic(bytes)),
    }
}

fn superblock_layout(bytes: &[u8], fs_uuid: &[u8; 16]) -> Option<MetadataLayout> {
    if bytes.get(248..264) == Some(fs_uuid.as_slice()) {
        Some(layout(224, 240, 248))
    } else {
        Some(layout(224, 240, 32))
    }
}

fn btree_layout(magic: u32) -> Option<MetadataLayout> {
    match magic {
        XFS_ABTB_CRC_MAGIC | XFS_ABTC_CRC_MAGIC | XFS_IBT_CRC_MAGIC | XFS_FIBT_CRC_MAGIC => {
            Some(layout(52, 24, 32))
        }
        XFS_BMAP_CRC_MAGIC => Some(layout(64, 32, 40)),
        _ => None,
    }
}

fn da_layout(bytes: &[u8], buffer_type: u16) -> Option<MetadataLayout> {
    let magic = be_u16(bytes, 8)?;
    let matches_type = matches!(
        (buffer_type, magic),
        (BLFT_DIR_LEAF1, XFS_DIR3_LEAF1_MAGIC)
            | (BLFT_DIR_LEAFN, XFS_DIR3_LEAFN_MAGIC)
            | (BLFT_DA_NODE, XFS_DA3_NODE_MAGIC)
            | (BLFT_ATTR_LEAF, XFS_ATTR3_LEAF_MAGIC)
    );
    matches_type.then(|| layout(12, 24, 32))
}

fn da_layout_from_magic(bytes: &[u8]) -> Option<MetadataLayout> {
    matches!(
        be_u16(bytes, 8)?,
        XFS_DIR3_LEAF1_MAGIC | XFS_DIR3_LEAFN_MAGIC | XFS_DA3_NODE_MAGIC | XFS_ATTR3_LEAF_MAGIC
    )
    .then(|| layout(12, 24, 32))
}

const fn layout(crc: usize, lsn: usize, uuid: usize) -> MetadataLayout {
    MetadataLayout { crc, lsn, uuid }
}

pub(crate) fn stamp_crc32c(bytes: &mut [u8], crc_offset: usize) {
    let mut crc = crc32c(u32::MAX, &bytes[..crc_offset]);
    crc = crc32c(crc, &[0; 4]);
    crc = crc32c(crc, &bytes[crc_offset + 4..]);
    bytes[crc_offset..crc_offset + 4].copy_from_slice(&(!crc).to_le_bytes());
}

fn be_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn be_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}
