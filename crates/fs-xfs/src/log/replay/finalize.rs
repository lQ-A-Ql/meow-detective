//! Finalize grouped BUF actions against the current volume image.

use super::{
    buffer, dinode, inode_buffer, ReplayDisposition, XfsBufferReplay, XfsInodeReplay, XfsLogError,
    XfsLogReplay, XfsReplayAction, XfsReplayFinal, XfsReplayPatch,
};

pub(crate) fn finalize_replay<F>(
    replay: XfsLogReplay,
    metadata_crc: bool,
    fs_uuid: &[u8; 16],
    mut read: F,
) -> Result<XfsReplayFinal, XfsLogError>
where
    F: FnMut(u64, usize) -> Result<Vec<u8>, XfsLogError>,
{
    let mut patches = Vec::new();
    debug_assert_eq!(replay.skipped_items, 0);
    for action in replay.actions {
        match action {
            XfsReplayAction::Patch(patch) => patches.push(patch),
            XfsReplayAction::Buffer(buffer) => {
                let _disposition =
                    finalize_buffer(buffer, metadata_crc, fs_uuid, &mut patches, &mut read)?;
            }
            XfsReplayAction::Inode(inode) => {
                let _disposition =
                    finalize_inode(inode, metadata_crc, fs_uuid, &mut patches, &mut read)?;
            }
        }
    }
    Ok(XfsReplayFinal {
        patches,
        skipped_items: 0,
    })
}

fn finalize_inode<F>(
    replay: XfsInodeReplay,
    metadata_crc: bool,
    fs_uuid: &[u8; 16],
    patches: &mut Vec<XfsReplayPatch>,
    read: &mut F,
) -> Result<ReplayDisposition, XfsLogError>
where
    F: FnMut(u64, usize) -> Result<Vec<u8>, XfsLogError>,
{
    let mut current = read(replay.offset, replay.length)?;
    if current.len() != replay.length {
        return Err(XfsLogError::InvalidData(
            "inode replay read returned the wrong length".into(),
        ));
    }
    overlay_prior_patches(&mut current, replay.offset, patches);
    validate_inode_identity(&current, replay.offset, replay.inode_number, fs_uuid)?;
    if metadata_crc {
        let current_lsn = be_u64(&current, 112);
        if current_lsn.is_some_and(|lsn| {
            lsn != 0 && lsn != u64::MAX && buffer::lsn_is_at_or_after(lsn, replay.lsn)
        }) {
            return Ok(ReplayDisposition::AlreadyCurrent);
        }
    }
    for write in &replay.writes {
        if !overlay_write(&mut current, replay.offset, write) {
            return unsafe_replay("INODE patch escapes its target object");
        }
    }
    validate_inode_identity(&current, replay.offset, replay.inode_number, fs_uuid).map_err(
        |_| XfsLogError::UnsafeReplay("INODE replay changed the object identity".into()),
    )?;
    if metadata_crc {
        dinode::stamp_metadata_crc(&mut current);
    }
    patches.push(XfsReplayPatch {
        offset: replay.offset,
        bytes: current,
    });
    Ok(ReplayDisposition::Applied)
}

fn validate_inode_identity(
    bytes: &[u8],
    offset: u64,
    inode_number: u64,
    fs_uuid: &[u8; 16],
) -> Result<(), XfsLogError> {
    let magic = bytes
        .get(0..2)
        .and_then(|value| value.try_into().ok())
        .map(u16::from_be_bytes);
    let version = bytes.get(4).copied();
    let actual_inode = be_u64(bytes, 152);
    let actual_uuid = bytes.get(160..176);
    if magic == Some(0x494E)
        && version == Some(3)
        && actual_inode == Some(inode_number)
        && actual_uuid == Some(fs_uuid.as_slice())
    {
        return Ok(());
    }
    unsafe_replay(format!(
        "INODE identity mismatch at volume offset {offset}: expected inode {inode_number}, \
         found magic {}, version {}, inode {}, uuid {}",
        option_hex_u16(magic),
        option_u8(version),
        option_u64(actual_inode),
        option_hex(actual_uuid),
    ))
}

fn option_hex_u16(value: Option<u16>) -> String {
    value
        .map(|value| format!("0x{value:04x}"))
        .unwrap_or_else(|| "truncated".to_string())
}

fn option_u8(value: Option<u8>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "truncated".to_string())
}

fn option_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "truncated".to_string())
}

fn option_hex(value: Option<&[u8]>) -> String {
    value
        .map(|bytes| bytes.iter().map(|byte| format!("{byte:02x}")).collect())
        .unwrap_or_else(|| "truncated".to_string())
}

fn be_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

fn finalize_buffer<F>(
    replay: XfsBufferReplay,
    metadata_crc: bool,
    fs_uuid: &[u8; 16],
    patches: &mut Vec<XfsReplayPatch>,
    read: &mut F,
) -> Result<ReplayDisposition, XfsLogError>
where
    F: FnMut(u64, usize) -> Result<Vec<u8>, XfsLogError>,
{
    let mut current = read(replay.offset, replay.length)?;
    if current.len() != replay.length {
        return Err(XfsLogError::InvalidData(
            "buffer replay read returned the wrong length".into(),
        ));
    }
    overlay_prior_patches(&mut current, replay.offset, patches);
    if matches!(replay.buffer_type, 1..=3) {
        return unsafe_replay("DQUOT buffer validation is not implemented");
    }
    if replay.inode_unlinked_only {
        if replay.buffer_type != 8 {
            return unsafe_replay("inode-unlinked BUF item is not typed as a DINO buffer");
        }
        inode_buffer::replay_unlinked_fields(&mut current, &replay, fs_uuid, metadata_crc)?;
        patches.push(XfsReplayPatch {
            offset: replay.offset,
            bytes: current,
        });
        return Ok(ReplayDisposition::Applied);
    }
    if metadata_crc
        && buffer::current_lsn(&current, fs_uuid)
            .is_some_and(|lsn| buffer::lsn_is_at_or_after(lsn, replay.lsn))
    {
        return Ok(ReplayDisposition::AlreadyCurrent);
    }
    for write in &replay.writes {
        if !overlay_write(&mut current, replay.offset, write) {
            return unsafe_replay("BUF patch escapes its target buffer");
        }
    }
    if replay.buffer_type == 8 {
        inode_buffer::validate_allocation_buffer(
            &current,
            replay.inode_size,
            replay.ag_inode_count,
            fs_uuid,
            metadata_crc,
        )?;
        patches.push(XfsReplayPatch {
            offset: replay.offset,
            bytes: current,
        });
        return Ok(ReplayDisposition::Applied);
    }
    if metadata_crc && !buffer::requires_verifier(replay.buffer_type) {
        return unsafe_replay(format!(
            "BUF type {} has no supported v5 write verifier",
            replay.buffer_type
        ));
    }
    if metadata_crc {
        buffer::seal(&mut current, replay.buffer_type, replay.lsn, fs_uuid).map_err(|reason| {
            XfsLogError::UnsafeReplay(format!(
                "BUF verifier rejected type {} at volume offset {}: {reason}",
                replay.buffer_type, replay.offset
            ))
        })?;
        patches.push(XfsReplayPatch {
            offset: replay.offset,
            bytes: current,
        });
    } else {
        patches.extend(replay.writes);
    }
    Ok(ReplayDisposition::Applied)
}

fn unsafe_replay<T>(message: impl Into<String>) -> Result<T, XfsLogError> {
    Err(XfsLogError::UnsafeReplay(message.into()))
}

fn overlay_prior_patches(target: &mut [u8], target_offset: u64, patches: &[XfsReplayPatch]) {
    let target_end = target_offset.saturating_add(target.len() as u64);
    for patch in patches {
        let patch_end = patch.offset.saturating_add(patch.bytes.len() as u64);
        let start = target_offset.max(patch.offset);
        let end = target_end.min(patch_end);
        if start >= end {
            continue;
        }
        let target_start = (start - target_offset) as usize;
        let patch_start = (start - patch.offset) as usize;
        let length = (end - start) as usize;
        target[target_start..target_start + length]
            .copy_from_slice(&patch.bytes[patch_start..patch_start + length]);
    }
}

fn overlay_write(target: &mut [u8], target_offset: u64, patch: &XfsReplayPatch) -> bool {
    let Some(relative) = patch.offset.checked_sub(target_offset) else {
        return false;
    };
    let Ok(start) = usize::try_from(relative) else {
        return false;
    };
    let Some(end) = start.checked_add(patch.bytes.len()) else {
        return false;
    };
    let Some(destination) = target.get_mut(start..end) else {
        return false;
    };
    destination.copy_from_slice(&patch.bytes);
    true
}
