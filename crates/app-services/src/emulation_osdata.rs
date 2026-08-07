//! Host-side OSDATA namespace removal for emulation sessions.
//!
//! A leftover `Windows/System32/config/OSDATA` node breaks direct boot of a
//! virtualized disk. This service plans the namespace edit with the read-only
//! NTFS analysis in `fs-ntfs` and applies the resulting byte replacements
//! through the session's copy-on-write overlay disk. The evidence image is
//! never written: the only writable parameter in this module is the session
//! `CowDisk`. A non-empty OSDATA directory is refused rather than partially
//! torn down.

use std::sync::Arc;

use evidence_emulation::CowDisk;
use transport::dto::{EmulationOsdataCleanupDto, EmulationOsdataCleanupStateDto};

use crate::emulation_bypass::{open_partition_filesystem, BypassCaseContext, EmulationBypassError};
use crate::emulation_cow_reader::CowDiskReader;

const CONFIG_DIR_PATH: &str = "Windows/System32/config";
const OSDATA_ENTRY_NAME: &str = "OSDATA";
const OSDATA_PATH: &str = "Windows/System32/config/OSDATA";

pub fn cleanup_osdata(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
) -> Result<EmulationOsdataCleanupDto, EmulationBypassError> {
    let context = open_partition_filesystem(case_context, partition_index)?;
    let data_source_id = case_context.data_source_id.0.clone();
    let result = |state, edits_applied| EmulationOsdataCleanupDto {
        session_id: String::new(),
        data_source_id: data_source_id.clone(),
        partition_index,
        state,
        edits_applied,
    };

    let Some(entry) = context
        .fs
        .list_subdir_children(CONFIG_DIR_PATH)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?
        .into_iter()
        .find(|node| node.name.eq_ignore_ascii_case(OSDATA_ENTRY_NAME))
    else {
        return Ok(result(EmulationOsdataCleanupStateDto::Absent, 0));
    };
    if entry.is_dir {
        let children = context
            .fs
            .list_subdir_children(OSDATA_PATH)
            .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
        if !children.is_empty() {
            return Ok(result(EmulationOsdataCleanupStateDto::RefusedNonEmpty, 0));
        }
    }

    let removal = context
        .fs
        .plan_directory_entry_removal(CONFIG_DIR_PATH, OSDATA_ENTRY_NAME)
        .map_err(|error| EmulationBypassError::Edit(error.to_string()))?;
    for edit in &removal.edits {
        let absolute = context
            .partition_offset
            .checked_add(edit.offset)
            .ok_or_else(|| EmulationBypassError::Edit("edit address overflows".to_string()))?;
        disk.write_all_at(absolute, &edit.bytes)
            .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))?;
    }
    disk.flush()
        .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))?;
    verify_removal(disk, &context, &removal.edits)?;

    let edits_applied = u32::try_from(removal.edits.len())
        .map_err(|_| EmulationBypassError::Edit("edit count overflows".to_string()))?;
    Ok(result(
        EmulationOsdataCleanupStateDto::Removed,
        edits_applied,
    ))
}

/// Confirm the edits landed and that the edited volume no longer lists the
/// entry — a semantic check through a fresh filesystem view over the overlay,
/// not just a byte read-back.
fn verify_removal(
    disk: &Arc<CowDisk>,
    context: &crate::emulation_bypass::PartitionFilesystem,
    edits: &[fs_ntfs::PlannedDiskEdit],
) -> Result<(), EmulationBypassError> {
    for edit in edits {
        let absolute = context
            .partition_offset
            .checked_add(edit.offset)
            .ok_or_else(|| EmulationBypassError::Edit("edit address overflows".to_string()))?;
        let mut readback = vec![0u8; edit.bytes.len()];
        disk.read_exact_at(absolute, &mut readback)
            .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))?;
        if readback != edit.bytes {
            return Err(EmulationBypassError::OverlayWrite(
                "overlay read-back does not match the planned edit".to_string(),
            ));
        }
    }
    let reader = CowDiskReader::new(Arc::clone(disk));
    let window =
        evidence_core::PartitionWindowReader::new(Box::new(reader), context.partition_offset, None)
            .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    let fs = fs_ntfs::NtfsReader::open(Box::new(window), 0)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
    let still_listed = fs
        .list_subdir_children(CONFIG_DIR_PATH)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?
        .into_iter()
        .any(|node| node.name.eq_ignore_ascii_case(OSDATA_ENTRY_NAME));
    if still_listed {
        return Err(EmulationBypassError::OverlayWrite(
            "the edited volume still lists the OSDATA entry".to_string(),
        ));
    }
    Ok(())
}
