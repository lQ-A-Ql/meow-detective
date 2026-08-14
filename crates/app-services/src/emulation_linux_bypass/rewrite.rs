use std::sync::Arc;

use evidence_emulation::CowDisk;

use super::volume::{map_xfs_rewrite_error, LinuxFilesystem, LinuxPartition, WriteMapping};
use super::SHADOW_PATH;
use crate::emulation_bypass::EmulationBypassError;

const I_SIZE_LO_OFFSET: u64 = 0x04;
const I_SIZE_HI_OFFSET: u64 = 0x6C;

pub(super) struct VolumePatch {
    pub(super) volume_offset: u64,
    pub(super) bytes: Vec<u8>,
}

pub(super) fn plan_shadow_rewrite(
    partition: &LinuxPartition,
    content: &[u8],
) -> Result<Vec<VolumePatch>, EmulationBypassError> {
    let patches = match &partition.fs {
        LinuxFilesystem::Ext4(fs) => plan_ext4_rewrite(fs, content)?,
        LinuxFilesystem::Xfs(fs) => fs
            .plan_in_place_file_rewrite(SHADOW_PATH, content)
            .map_err(map_xfs_rewrite_error)?
            .patches
            .into_iter()
            .map(|patch| (patch.volume_offset, patch.bytes))
            .collect(),
    };
    Ok(patches
        .into_iter()
        .map(|(volume_offset, bytes)| VolumePatch {
            volume_offset,
            bytes,
        })
        .collect())
}

fn plan_ext4_rewrite(
    fs: &fs_ext4::Ext4Reader,
    content: &[u8],
) -> Result<Vec<(u64, Vec<u8>)>, EmulationBypassError> {
    let old_len = fs
        .file_size_by_path(SHADOW_PATH)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    if content.len() as u64 > old_len {
        return Err(EmulationBypassError::Unsupported(
            "the edited shadow file cannot grow in place".to_string(),
        ));
    }
    let extents = fs
        .file_extent_map(SHADOW_PATH)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
    let mut patches = extent_range_patches(&extents, 0, content.len() as u64, Some(content))?;
    patches.extend(extent_range_patches(
        &extents,
        content.len() as u64,
        old_len,
        None,
    )?);
    let new_len = u32::try_from(content.len())
        .map_err(|_| EmulationBypassError::Edit("edited shadow exceeds u32 i_size".into()))?;
    let inode_offset = fs
        .inode_source_offset(SHADOW_PATH)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
    patches.push((
        inode_offset + I_SIZE_LO_OFFSET,
        new_len.to_le_bytes().to_vec(),
    ));
    patches.push((inode_offset + I_SIZE_HI_OFFSET, 0u32.to_le_bytes().to_vec()));
    Ok(patches)
}

fn extent_range_patches(
    extents: &[fs_ext4::Ext4FileExtent],
    range_start: u64,
    range_end: u64,
    source: Option<&[u8]>,
) -> Result<Vec<(u64, Vec<u8>)>, EmulationBypassError> {
    let mut patches = Vec::new();
    for extent in extents {
        let extent_end = extent
            .logical_offset
            .checked_add(extent.length)
            .ok_or_else(|| EmulationBypassError::Edit("extent range overflows".into()))?;
        let start = extent.logical_offset.max(range_start);
        let end = extent_end.min(range_end);
        if start >= end {
            continue;
        }
        let volume_offset = extent
            .volume_offset
            .checked_add(start - extent.logical_offset)
            .ok_or_else(|| EmulationBypassError::Edit("extent address overflows".into()))?;
        let bytes = match source {
            Some(bytes) => bytes[start as usize..end as usize].to_vec(),
            None => vec![0; (end - start) as usize],
        };
        patches.push((volume_offset, bytes));
    }
    Ok(patches)
}

pub(super) fn validate_rewrite_plan(
    mapping: &WriteMapping,
    patches: &[VolumePatch],
) -> Result<(), EmulationBypassError> {
    for patch in patches {
        let mut mapped = 0usize;
        while mapped < patch.bytes.len() {
            let (_, run) = mapping.translate_run(patch.volume_offset + mapped as u64)?;
            let chunk = usize::try_from(run)
                .unwrap_or(usize::MAX)
                .min(patch.bytes.len() - mapped);
            if chunk == 0 {
                return Err(EmulationBypassError::Edit(
                    "rewrite patch maps to a zero-length gap".to_string(),
                ));
            }
            mapped += chunk;
        }
    }
    Ok(())
}

pub(super) fn apply_rewrite_plan(
    disk: &Arc<CowDisk>,
    mapping: &WriteMapping,
    patches: &[VolumePatch],
) -> Result<(), EmulationBypassError> {
    for patch in patches {
        write_patch(disk, mapping, patch)?;
    }
    disk.flush()
        .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))
}

fn write_patch(
    disk: &Arc<CowDisk>,
    mapping: &WriteMapping,
    patch: &VolumePatch,
) -> Result<(), EmulationBypassError> {
    let mut written = 0usize;
    while written < patch.bytes.len() {
        let (absolute, run) = mapping.translate_run(patch.volume_offset + written as u64)?;
        let chunk = usize::try_from(run)
            .unwrap_or(usize::MAX)
            .min(patch.bytes.len() - written);
        disk.write_all_at(absolute, &patch.bytes[written..written + chunk])
            .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))?;
        written += chunk;
    }
    Ok(())
}

pub(super) fn verify_patch_bytes(
    disk: &Arc<CowDisk>,
    mapping: &WriteMapping,
    patches: &[VolumePatch],
) -> Result<(), EmulationBypassError> {
    for patch in patches {
        verify_patch(disk, mapping, patch)?;
    }
    Ok(())
}

fn verify_patch(
    disk: &Arc<CowDisk>,
    mapping: &WriteMapping,
    patch: &VolumePatch,
) -> Result<(), EmulationBypassError> {
    let mut verified = 0usize;
    while verified < patch.bytes.len() {
        let (absolute, run) = mapping.translate_run(patch.volume_offset + verified as u64)?;
        let chunk = usize::try_from(run)
            .unwrap_or(usize::MAX)
            .min(patch.bytes.len() - verified);
        let mut actual = vec![0; chunk];
        disk.read_exact_at(absolute, &mut actual)
            .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))?;
        if actual != patch.bytes[verified..verified + chunk] {
            return Err(EmulationBypassError::OverlayWrite(
                "overlay rewrite patch failed byte-for-byte verification".to_string(),
            ));
        }
        verified += chunk;
    }
    Ok(())
}
