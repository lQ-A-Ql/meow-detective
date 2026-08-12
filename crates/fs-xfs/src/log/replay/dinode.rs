//! Conversion of host-order log dinode cores to on-disk v3 dinodes.
//!
//! Mirrors the kernel's `xfs_log_dinode_to_disk`
//! (fs/xfs/xfs_inode_item_recover.c): every field of the 176-byte
//! `xfs_log_dinode` core is read in the log's host byte order and written
//! big-endian at the same offset, `di_lsn` is replaced by the recovering
//! transaction's LSN, the v3 padding unions are zeroed, and `di_crc` is
//! resealed with the v5 metadata CRC32C.

use super::super::checksum::crc32c;
use super::super::{XfsLogError, XfsLogFormat};

pub(super) const V3_CORE_SIZE: usize = 176;
/// `offsetof(struct xfs_dinode, di_crc)`; the CRC field itself is `__le32`.
const CRC_OFFSET: usize = 100;
const LSN_OFFSET: usize = 112;
const FLAGS2_OFFSET: usize = 120;
const DIFLAG2_BIGTIME: u64 = 1 << 3;
const DIFLAG2_NREXT64: u64 = 1 << 4;
const DINODE_MAGIC: u16 = 0x494E;
const NULLAGINO: u32 = u32::MAX;

/// Reseal a v5 metadata object whose CRC field sits at offset 100 with the
/// formula validated against the reference image: plain `~0` seed, four zero
/// bytes in place of the CRC field, one's complement, little-endian store.
pub(super) fn stamp_metadata_crc(object: &mut [u8]) {
    object[CRC_OFFSET..CRC_OFFSET + 4].fill(0);
    let mut crc = crc32c(u32::MAX, &object[..CRC_OFFSET]);
    crc = crc32c(crc, &[0u8; 4]);
    crc = crc32c(crc, &object[CRC_OFFSET + 4..]);
    object[CRC_OFFSET..CRC_OFFSET + 4].copy_from_slice(&(!crc).to_le_bytes());
}

/// Convert a 176-byte v3 log dinode core into its on-disk form, stamping
/// `di_lsn` with `lsn` and recomputing `di_crc`.
pub(super) fn log_core_to_disk(
    format: XfsLogFormat,
    core: &[u8],
    lsn: u64,
) -> Result<[u8; V3_CORE_SIZE], XfsLogError> {
    if core.len() != V3_CORE_SIZE {
        return Err(invalid(format!(
            "log dinode core has {} bytes; expected {V3_CORE_SIZE}",
            core.len()
        )));
    }
    if native_u16(format, core, 0)? != DINODE_MAGIC {
        return Err(invalid("log dinode core has invalid XFS inode magic"));
    }
    if core[4] != 3 {
        return Err(invalid(format!(
            "log dinode core has unsupported version {}",
            core[4]
        )));
    }
    let flags2 = native_u64(format, core, FLAGS2_OFFSET)?;
    let bigtime = flags2 & DIFLAG2_BIGTIME != 0;
    let mut out = [0u8; V3_CORE_SIZE];
    be16(format, core, &mut out, 0)?; // di_magic
    be16(format, core, &mut out, 2)?; // di_mode
    out[4] = core[4]; // di_version
    out[5] = core[5]; // di_format
    be16(format, core, &mut out, 6)?; // di_metatype
    be32(format, core, &mut out, 8)?; // di_uid
    be32(format, core, &mut out, 12)?; // di_gid
    be32(format, core, &mut out, 16)?; // di_nlink
    be16(format, core, &mut out, 20)?; // di_projid_lo
    be16(format, core, &mut out, 22)?; // di_projid_hi
    timestamp(format, core, &mut out, 32, bigtime)?; // di_atime
    timestamp(format, core, &mut out, 40, bigtime)?; // di_mtime
    timestamp(format, core, &mut out, 48, bigtime)?; // di_ctime
    be64(format, core, &mut out, 56)?; // di_size
    be64(format, core, &mut out, 64)?; // di_nblocks
    be32(format, core, &mut out, 72)?; // di_extsize
    extent_counts(format, core, &mut out, flags2)?;
    out[82] = core[82]; // di_forkoff
    out[83] = core[83]; // di_aformat
    be32(format, core, &mut out, 84)?; // di_dmevmask
    be16(format, core, &mut out, 88)?; // di_dmstate
    be16(format, core, &mut out, 90)?; // di_flags
    be32(format, core, &mut out, 92)?; // di_gen
    be32(format, core, &mut out, 96)?; // di_next_unlinked
    be64(format, core, &mut out, 104)?; // di_changecount
    out[LSN_OFFSET..LSN_OFFSET + 8].copy_from_slice(&lsn.to_be_bytes());
    out[FLAGS2_OFFSET..FLAGS2_OFFSET + 8].copy_from_slice(&flags2.to_be_bytes());
    be32(format, core, &mut out, 128)?; // di_cowextsize
    timestamp(format, core, &mut out, 144, bigtime)?; // di_crtime
    be64(format, core, &mut out, 152)?; // di_ino
    out[160..176].copy_from_slice(&core[160..176]); // di_uuid
    stamp_metadata_crc(&mut out);
    Ok(out)
}

/// The inode image the kernel's `xfs_ialloc_inode_init` writes for an
/// ICREATE recovery: a zeroed v3 inode carrying only the magic, version,
/// generation, NULLAGINO unlink pointer, inode number, filesystem UUID and a
/// valid CRC. `di_lsn` stays zero, which every LSN verifier treats as
/// "no LSN" and therefore always accepts.
pub(super) fn fresh_v3_inode(
    inode_size: usize,
    generation: u32,
    ino: u64,
    fs_uuid: &[u8; 16],
) -> Vec<u8> {
    let mut inode = vec![0u8; inode_size.max(V3_CORE_SIZE)];
    inode[0..2].copy_from_slice(&DINODE_MAGIC.to_be_bytes());
    inode[4] = 3;
    inode[92..96].copy_from_slice(&generation.to_be_bytes());
    inode[96..100].copy_from_slice(&NULLAGINO.to_be_bytes());
    inode[152..160].copy_from_slice(&ino.to_be_bytes());
    inode[160..176].copy_from_slice(fs_uuid);
    stamp_metadata_crc(&mut inode);
    inode
}

/// The 8-byte log timestamp is `(i32 sec, i32 nsec)` in host order; the
/// on-disk legacy timestamp stores both counters big-endian. With the
/// bigtime feature the whole word is a be64 nanosecond counter.
fn timestamp(
    format: XfsLogFormat,
    core: &[u8],
    out: &mut [u8; V3_CORE_SIZE],
    offset: usize,
    bigtime: bool,
) -> Result<(), XfsLogError> {
    if bigtime {
        return be64(format, core, out, offset);
    }
    be32(format, core, out, offset)?;
    be32(format, core, out, offset + 4)
}

/// The extent-count unions differ between the classic and NREXT64 layouts;
/// everything else shares the same offsets. Bytes 24..32 stay zero for the
/// classic layout (`di_v3_pad`), matching the kernel.
fn extent_counts(
    format: XfsLogFormat,
    core: &[u8],
    out: &mut [u8; V3_CORE_SIZE],
    flags2: u64,
) -> Result<(), XfsLogError> {
    if flags2 & DIFLAG2_NREXT64 != 0 {
        be64(format, core, out, 24)?; // di_big_nextents
        be32(format, core, out, 76)?; // di_big_anextents
        be16(format, core, out, 80) // di_nrext64_pad
    } else {
        be32(format, core, out, 76)?; // di_nextents
        be16(format, core, out, 80) // di_anextents
    }
}

fn be16(
    format: XfsLogFormat,
    core: &[u8],
    out: &mut [u8; V3_CORE_SIZE],
    offset: usize,
) -> Result<(), XfsLogError> {
    out[offset..offset + 2].copy_from_slice(&native_u16(format, core, offset)?.to_be_bytes());
    Ok(())
}

fn be32(
    format: XfsLogFormat,
    core: &[u8],
    out: &mut [u8; V3_CORE_SIZE],
    offset: usize,
) -> Result<(), XfsLogError> {
    out[offset..offset + 4].copy_from_slice(
        &format
            .native_u32(core, offset)
            .ok_or_else(|| invalid(format!("cannot decode native u32 at byte {offset}")))?
            .to_be_bytes(),
    );
    Ok(())
}

fn be64(
    format: XfsLogFormat,
    core: &[u8],
    out: &mut [u8; V3_CORE_SIZE],
    offset: usize,
) -> Result<(), XfsLogError> {
    out[offset..offset + 8].copy_from_slice(
        &format
            .native_u64(core, offset)
            .ok_or_else(|| invalid(format!("cannot decode native u64 at byte {offset}")))?
            .to_be_bytes(),
    );
    Ok(())
}

fn native_u16(format: XfsLogFormat, core: &[u8], offset: usize) -> Result<u16, XfsLogError> {
    format
        .native_u16(core, offset)
        .ok_or_else(|| invalid(format!("cannot decode native u16 at byte {offset}")))
}

fn native_u64(format: XfsLogFormat, core: &[u8], offset: usize) -> Result<u64, XfsLogError> {
    format
        .native_u64(core, offset)
        .ok_or_else(|| invalid(format!("cannot decode native u64 at byte {offset}")))
}

fn invalid(message: impl Into<String>) -> XfsLogError {
    XfsLogError::InvalidData(message.into())
}
