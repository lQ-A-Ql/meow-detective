//! Recovery of inode-buffer items that log only `di_next_unlinked` fields.

use super::dinode;
use super::{XfsBufferReplay, XfsLogError, XfsReplayPatch};

const DINODE_MAGIC: u16 = 0x494E;
const DINODE_VERSION_3: u8 = 3;
const NEXT_UNLINKED_OFFSET: usize = 96;
const INODE_UUID_OFFSET: usize = 160;
const INODE_UUID_END: usize = 176;
const NULLAGINO: u32 = u32::MAX;

pub(super) fn replay_unlinked_fields(
    current: &mut [u8],
    replay: &XfsBufferReplay,
    fs_uuid: &[u8; 16],
    metadata_crc: bool,
) -> Result<(), XfsLogError> {
    let inode_size = usize::from(replay.inode_size);
    if inode_size < INODE_UUID_END || !current.len().is_multiple_of(inode_size) {
        return unsafe_replay("DINO buffer length is not a whole number of inodes");
    }
    for inode_offset in (0..current.len()).step_by(inode_size) {
        let inode = &mut current[inode_offset..inode_offset + inode_size];
        validate_inode(inode, replay.ag_inode_count, fs_uuid, metadata_crc)?;
        let field_offset = inode_offset + NEXT_UNLINKED_OFFSET;
        if let Some(value) = logged_unlinked(&replay.writes, replay.offset, field_offset)? {
            validate_unlinked(value, replay.ag_inode_count)?;
            inode[NEXT_UNLINKED_OFFSET..NEXT_UNLINKED_OFFSET + 4]
                .copy_from_slice(&value.to_be_bytes());
            if metadata_crc {
                dinode::stamp_metadata_crc(inode);
            }
        }
    }
    Ok(())
}

pub(super) fn validate_allocation_buffer(
    current: &[u8],
    inode_size: u16,
    ag_inode_count: u64,
    fs_uuid: &[u8; 16],
    metadata_crc: bool,
) -> Result<(), XfsLogError> {
    let inode_size = usize::from(inode_size);
    let minimum_inode_size = if metadata_crc {
        INODE_UUID_END
    } else {
        NEXT_UNLINKED_OFFSET + 4
    };
    if inode_size < minimum_inode_size || !current.len().is_multiple_of(inode_size) {
        return unsafe_replay("DINO allocation buffer length is not a whole number of inodes");
    }
    for inode in current.chunks_exact(inode_size) {
        validate_inode(inode, ag_inode_count, fs_uuid, metadata_crc)?;
        if metadata_crc && !dinode::metadata_crc_is_valid(inode) {
            return unsafe_replay("DINO allocation buffer contains an invalid inode CRC");
        }
    }
    Ok(())
}

fn logged_unlinked(
    writes: &[XfsReplayPatch],
    buffer_offset: u64,
    field_offset: usize,
) -> Result<Option<u32>, XfsLogError> {
    let field_start = buffer_offset + field_offset as u64;
    let field_end = field_start + 4;
    let mut value = None;
    for write in writes {
        let write_end = write.offset.saturating_add(write.bytes.len() as u64);
        if write.offset >= field_end || write_end <= field_start {
            continue;
        }
        if write.offset > field_start || write_end < field_end || value.is_some() {
            return unsafe_replay(
                "DINO logged regions partially or repeatedly cover an unlink field",
            );
        }
        let start = usize::try_from(field_start - write.offset)
            .map_err(|_| XfsLogError::InvalidData("DINO field offset overflows".to_string()))?;
        value = Some(u32::from_be_bytes(
            write.bytes[start..start + 4]
                .try_into()
                .map_err(|_| XfsLogError::InvalidData("DINO unlink field is truncated".into()))?,
        ));
    }
    Ok(value)
}

fn validate_inode(
    inode: &[u8],
    ag_inode_count: u64,
    fs_uuid: &[u8; 16],
    metadata_crc: bool,
) -> Result<(), XfsLogError> {
    let magic = u16::from_be_bytes(inode[0..2].try_into().unwrap_or_default());
    let version_is_valid = if metadata_crc {
        inode[4] == DINODE_VERSION_3
    } else {
        matches!(inode[4], 1 | 2)
    };
    if magic != DINODE_MAGIC || !version_is_valid {
        return unsafe_replay("DINO buffer contains an invalid inode magic or version");
    }
    if metadata_crc && inode[INODE_UUID_OFFSET..INODE_UUID_END] != *fs_uuid {
        return unsafe_replay("DINO buffer contains an inode from another filesystem");
    }
    let next = u32::from_be_bytes(
        inode[NEXT_UNLINKED_OFFSET..NEXT_UNLINKED_OFFSET + 4]
            .try_into()
            .unwrap_or_default(),
    );
    validate_unlinked(next, ag_inode_count)
}

fn validate_unlinked(value: u32, ag_inode_count: u64) -> Result<(), XfsLogError> {
    if value != NULLAGINO && (value == 0 || u64::from(value) >= ag_inode_count) {
        return unsafe_replay("DINO buffer contains an invalid di_next_unlinked value");
    }
    Ok(())
}

fn unsafe_replay<T>(message: impl Into<String>) -> Result<T, XfsLogError> {
    Err(XfsLogError::UnsafeReplay(message.into()))
}
