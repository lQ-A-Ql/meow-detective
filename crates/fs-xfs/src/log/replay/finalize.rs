//! Finalize grouped BUF actions against the current volume image.

use super::{
    buffer, dinode, XfsBufferReplay, XfsInodeReplay, XfsLogError, XfsLogReplay, XfsReplayAction,
    XfsReplayFinal, XfsReplayPatch,
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
    let mut skipped_items = replay.skipped_items;
    for action in replay.actions {
        match action {
            XfsReplayAction::Patch(patch) => patches.push(patch),
            XfsReplayAction::Buffer(buffer) => {
                if !finalize_buffer(buffer, metadata_crc, fs_uuid, &mut patches, &mut read)? {
                    skipped_items = skipped_items.saturating_add(1);
                }
            }
            XfsReplayAction::Inode(inode) => {
                if !finalize_inode(inode, metadata_crc, fs_uuid, &mut patches, &mut read)? {
                    skipped_items = skipped_items.saturating_add(1);
                }
            }
        }
    }
    Ok(XfsReplayFinal {
        patches,
        skipped_items,
    })
}

fn finalize_inode<F>(
    replay: XfsInodeReplay,
    metadata_crc: bool,
    fs_uuid: &[u8; 16],
    patches: &mut Vec<XfsReplayPatch>,
    read: &mut F,
) -> Result<bool, XfsLogError>
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
    if !valid_inode_identity(&current, replay.inode_number, fs_uuid) {
        return Ok(false);
    }
    if metadata_crc {
        let current_lsn = be_u64(&current, 112);
        if current_lsn.is_some_and(|lsn| {
            lsn != 0 && lsn != u64::MAX && buffer::lsn_is_at_or_after(lsn, replay.lsn)
        }) {
            return Ok(true);
        }
    }
    for write in &replay.writes {
        if !overlay_write(&mut current, replay.offset, write) {
            return Ok(false);
        }
    }
    if !valid_inode_identity(&current, replay.inode_number, fs_uuid) {
        return Ok(false);
    }
    if metadata_crc {
        dinode::stamp_metadata_crc(&mut current);
    }
    patches.push(XfsReplayPatch {
        offset: replay.offset,
        bytes: current,
    });
    Ok(true)
}

fn valid_inode_identity(bytes: &[u8], inode_number: u64, fs_uuid: &[u8; 16]) -> bool {
    bytes.get(0..2) == Some(0x494Eu16.to_be_bytes().as_slice())
        && bytes.get(4) == Some(&3)
        && be_u64(bytes, 152) == Some(inode_number)
        && bytes.get(160..176) == Some(fs_uuid.as_slice())
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
) -> Result<bool, XfsLogError>
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
    if metadata_crc
        && buffer::current_lsn(&current, fs_uuid)
            .is_some_and(|lsn| buffer::lsn_is_at_or_after(lsn, replay.lsn))
    {
        return Ok(true);
    }
    for write in &replay.writes {
        if !overlay_write(&mut current, replay.offset, write) {
            return Ok(false);
        }
    }
    if metadata_crc && buffer::requires_verifier(replay.buffer_type) {
        if !buffer::seal(&mut current, replay.buffer_type, replay.lsn, fs_uuid) {
            return Ok(false);
        }
        patches.push(XfsReplayPatch {
            offset: replay.offset,
            bytes: current,
        });
    } else {
        patches.extend(replay.writes);
    }
    Ok(true)
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
