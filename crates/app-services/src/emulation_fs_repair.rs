//! Host-side XFS log-clear repair for emulation sessions.
//!
//! A forensic image captured from a running system carries a dirty XFS
//! log. The guest kernel can replay it, but crash-truncated user-space
//! files then break the booted system (observed on the CentOS 7 sample:
//! dbus/logind cascade, no login prompt). This service applies the
//! repair plan computed by `fs-xfs` through the session COW overlay:
//! zero the internal log, terminate it with the mkfs-style dummy unmount
//! record, and normalize the LSN (+CRC32C) of every metadata object the
//! mount path verifies (superblock, per-AG AGF/AGI, root/realtime
//! inodes). The evidence image is never written; only volumes whose
//! assessment reports `Dirty` are touched.

use std::sync::Arc;

use evidence_core::{EvidenceReader, PartitionWindowReader};
use evidence_emulation::CowDisk;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo,
    partition_repo::{DataSourcePartitionRecord, PartitionRepo},
};
use transport::dto::{
    EmulationFsRepairItemDto, EmulationFsRepairResultDto, EmulationFsVolumeStateDto,
};

use crate::emulation_bypass::{BypassCaseContext, EmulationBypassError};
use crate::emulation_cow_reader::CowDiskReader;

const REPAIR_WRITE_CHUNK_BYTES: usize = 8 * 1024 * 1024;

/// A located XFS volume plus the translation from volume-relative offsets
/// to absolute disk offsets.
struct XfsVolume {
    fs: fs_xfs::XfsReader,
    mapping: VolumeMapping,
}

struct PlannedVolume {
    record: DataSourcePartitionRecord,
    volume: XfsVolume,
    plan: Option<fs_xfs::XfsLogClearPlan>,
    log_bytes: u64,
}

enum VolumeAssessment {
    Ready(PlannedVolume),
    Unsupported(EmulationFsRepairItemDto),
}

enum VolumeMapping {
    Direct {
        partition_offset: u64,
        partition_length: u64,
    },
    /// LVM extent physical offsets are already disk-absolute in the
    /// coordinate space of the reader the pool was discovered from.
    Lvm { extents: Vec<fs_lvm::LvExtent> },
}

impl VolumeMapping {
    /// Translate `volume_offset` to an absolute offset plus the contiguous
    /// run length the mapping covers from it.
    fn translate_run(&self, volume_offset: u64) -> Result<(u64, u64), EmulationBypassError> {
        match self {
            Self::Direct {
                partition_offset,
                partition_length,
            } => {
                if volume_offset >= *partition_length {
                    return Err(EmulationBypassError::Edit(
                        "repair write starts beyond the partition end".to_string(),
                    ));
                }
                let absolute = partition_offset
                    .checked_add(volume_offset)
                    .ok_or_else(|| EmulationBypassError::Edit("address overflow".into()))?;
                Ok((absolute, partition_length - volume_offset))
            }
            Self::Lvm { extents } => {
                let index = extents
                    .partition_point(|extent| extent.logical_start <= volume_offset)
                    .checked_sub(1)
                    .ok_or_else(|| {
                        EmulationBypassError::Edit("offset below the LV extent map".into())
                    })?;
                let extent = &extents[index];
                let extent_end = extent
                    .logical_start
                    .checked_add(extent.length)
                    .ok_or_else(|| EmulationBypassError::Edit("LV extent overflow".into()))?;
                if volume_offset >= extent_end {
                    return Err(EmulationBypassError::Edit(
                        "offset is not covered by the LV extent map".to_string(),
                    ));
                }
                let absolute = extent
                    .physical_offset
                    .checked_add(volume_offset - extent.logical_start)
                    .ok_or_else(|| EmulationBypassError::Edit("address overflow".into()))?;
                Ok((absolute, extent_end - volume_offset))
            }
        }
    }
}

fn open_lv(
    disk: &Arc<CowDisk>,
    record: &DataSourcePartitionRecord,
) -> Result<fs_lvm::LvReader, EmulationBypassError> {
    let pv_offsets: Vec<u64> = record
        .lvm_pv_offsets_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .filter(|offsets: &Vec<u64>| !offsets.is_empty())
        .ok_or_else(|| {
            EmulationBypassError::Unsupported("LV has no persisted PV offsets".to_string())
        })?;
    // One overlay reader per PV, even when every PV lives on this disk.
    let readers = pv_offsets
        .iter()
        .map(|_| Box::new(CowDiskReader::new(Arc::clone(disk))) as Box<dyn EvidenceReader>)
        .collect();
    let pool = fs_lvm::LvmPool::discover(readers, pv_offsets)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
    let volumes = pool.list_volumes();
    let lv_index = volumes
        .iter()
        .position(|volume| {
            Some(volume.name.as_str()) == record.lvm_lv_name.as_deref()
                && Some(volume.uuid.as_str()) == record.lvm_lv_uuid.as_deref()
        })
        .ok_or_else(|| EmulationBypassError::PartitionNotFound {
            partition_index: record.partition_index,
        })?;
    pool.open_volume(lv_index)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))
}

/// Open an XFS volume through the session overlay, so assessment observes
/// repairs already applied in this session.
fn open_xfs_volume(
    disk: &Arc<CowDisk>,
    record: &DataSourcePartitionRecord,
) -> Result<XfsVolume, EmulationBypassError> {
    if record.lvm_lv_name.is_some() {
        let lv = open_lv(disk, record)?;
        let extents = lv.extent_map().to_vec();
        let fs = fs_xfs::XfsReader::open(Box::new(lv), 0)
            .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
        Ok(XfsVolume {
            fs,
            mapping: VolumeMapping::Lvm { extents },
        })
    } else {
        if record.length == 0 {
            return Err(EmulationBypassError::Unsupported(
                "partition has no declared length; writes cannot be bounded".to_string(),
            ));
        }
        let reader: Box<dyn EvidenceReader> = Box::new(CowDiskReader::new(Arc::clone(disk)));
        let window = PartitionWindowReader::new(reader, record.offset, Some(record.length))
            .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
        let fs = fs_xfs::XfsReader::open(Box::new(window), 0)
            .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
        Ok(XfsVolume {
            fs,
            mapping: VolumeMapping::Direct {
                partition_offset: record.offset,
                partition_length: record.length,
            },
        })
    }
}

fn apply_patch(
    disk: &Arc<CowDisk>,
    mapping: &VolumeMapping,
    patch: &fs_xfs::XfsRepairPatch,
) -> Result<(), EmulationBypassError> {
    let mut written = 0u64;
    while (written as usize) < patch.bytes.len() {
        let (absolute, run) = mapping.translate_run(patch.offset + written)?;
        let remaining = patch.bytes.len() - written as usize;
        let chunk = (run as usize).min(remaining).min(REPAIR_WRITE_CHUNK_BYTES);
        if chunk == 0 {
            return Err(EmulationBypassError::Edit(
                "repair patch maps to a zero-length gap".to_string(),
            ));
        }
        disk.write_all_at(
            absolute,
            &patch.bytes[written as usize..written as usize + chunk],
        )
        .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))?;
        written += chunk as u64;
    }
    Ok(())
}

fn validate_patch_mapping(
    mapping: &VolumeMapping,
    patch: &fs_xfs::XfsRepairPatch,
) -> Result<(), EmulationBypassError> {
    let mut mapped = 0u64;
    while (mapped as usize) < patch.bytes.len() {
        let (_, run) = mapping.translate_run(patch.offset + mapped)?;
        let remaining = patch.bytes.len() - mapped as usize;
        let chunk = (run as usize).min(remaining);
        if chunk == 0 {
            return Err(EmulationBypassError::Edit(
                "repair patch maps to a zero-length gap".to_string(),
            ));
        }
        mapped += chunk as u64;
    }
    Ok(())
}

/// Assess every XFS volume before applying any patch. If one volume cannot be
/// safely planned, every volume remains untouched and the unsupported state
/// is returned to the caller. This prevents a multi-volume source from being
/// left half-repaired merely because a later volume uses unsupported replay
/// semantics.
pub fn repair_xfs_logs(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
) -> Result<EmulationFsRepairResultDto, EmulationBypassError> {
    DataSourceRepo::new(case_context.case_conn)
        .find_by_case(case_context.case_id)?
        .into_iter()
        .find(|candidate| candidate.id == *case_context.data_source_id)
        .ok_or(EmulationBypassError::PartitionNotFound { partition_index: 0 })?;
    let source = crate::source_db::open_ready_source_read_only_by_id(
        case_context.case_conn,
        case_context.case_root,
        case_context.case_id,
        case_context.data_source_id,
    )
    .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    let records = PartitionRepo::new(&source.connection)
        .find_by_data_source(&case_context.data_source_id.0)?;

    let mut assessments = Vec::new();
    for record in records
        .into_iter()
        .filter(|record| record.filesystem.as_deref() == Some("XFS"))
    {
        assessments.push(plan_one_volume(disk, record)?);
    }
    if assessments
        .iter()
        .any(|assessment| matches!(assessment, VolumeAssessment::Unsupported(_)))
    {
        let items = assessments
            .into_iter()
            .map(assessment_item_without_writes)
            .collect();
        return Ok(repair_result(case_context, items));
    }

    let mut planned = assessments
        .into_iter()
        .filter_map(|assessment| match assessment {
            VolumeAssessment::Ready(volume) => Some(volume),
            VolumeAssessment::Unsupported(_) => None,
        })
        .collect::<Vec<_>>();
    for volume in &planned {
        validate_volume_plan(volume)?;
    }
    for volume in &planned {
        if let Err(error) = apply_volume_plan(disk, volume) {
            disk.invalidate();
            return Err(error);
        }
    }
    if let Err(error) = disk.flush() {
        disk.invalidate();
        return Err(EmulationBypassError::OverlayWrite(error.to_string()));
    }
    for volume in &planned {
        if let Err(error) = verify_volume(disk, volume) {
            disk.invalidate();
            return Err(error);
        }
    }
    let items = planned.drain(..).map(repaired_item).collect();
    Ok(repair_result(case_context, items))
}

fn validate_volume_plan(volume: &PlannedVolume) -> Result<(), EmulationBypassError> {
    if let Some(plan) = &volume.plan {
        for patch in &plan.patches {
            validate_patch_mapping(&volume.volume.mapping, patch)?;
        }
    }
    Ok(())
}

fn plan_one_volume(
    disk: &Arc<CowDisk>,
    record: DataSourcePartitionRecord,
) -> Result<VolumeAssessment, EmulationBypassError> {
    let partition_index = record.partition_index;
    let volume = match open_xfs_volume(disk, &record) {
        Ok(volume) => volume,
        Err(error) => {
            tracing::warn!(partition_index, error = %error, "xfs log repair: volume skipped");
            return Ok(VolumeAssessment::Unsupported(item(
                partition_index,
                EmulationFsVolumeStateDto::Unsupported,
                false,
                0,
            )));
        }
    };
    let log_bytes = volume
        .fs
        .log_geometry()
        .log_bytes()
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    let plan = match volume.fs.plan_log_repair() {
        Ok(plan) => plan,
        Err(error) => {
            tracing::warn!(partition_index, error = %error, "xfs log repair: planning failed");
            return Ok(VolumeAssessment::Unsupported(item(
                partition_index,
                EmulationFsVolumeStateDto::Unsupported,
                false,
                0,
            )));
        }
    };
    if plan.as_ref().is_some_and(|plan| plan.skipped_items != 0) {
        return Err(EmulationBypassError::Unsupported(format!(
            "partition {partition_index} produced a non-zero skipped replay count"
        )));
    }
    Ok(VolumeAssessment::Ready(PlannedVolume {
        record,
        volume,
        plan,
        log_bytes,
    }))
}

fn apply_volume_plan(
    disk: &Arc<CowDisk>,
    volume: &PlannedVolume,
) -> Result<(), EmulationBypassError> {
    let Some(plan) = &volume.plan else {
        return Ok(());
    };
    for patch in &plan.patches {
        apply_patch(disk, &volume.volume.mapping, patch)?;
    }
    Ok(())
}

fn verify_volume(disk: &Arc<CowDisk>, volume: &PlannedVolume) -> Result<(), EmulationBypassError> {
    if volume.plan.is_none() {
        return Ok(());
    }
    let verified = open_xfs_volume(disk, &volume.record)?;
    let snapshot = verified
        .fs
        .read_internal_log_snapshot(fs_xfs::log::XFS_LOG_MAX_SNAPSHOT_BYTES)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    if assess(&snapshot) != fs_xfs::log::XfsLogState::Clean {
        return Err(EmulationBypassError::OverlayWrite(
            "the repaired log does not assess as clean".to_string(),
        ));
    }
    Ok(())
}

fn assessment_item_without_writes(assessment: VolumeAssessment) -> EmulationFsRepairItemDto {
    match assessment {
        VolumeAssessment::Ready(volume) => item(
            volume.record.partition_index,
            if volume.plan.is_some() {
                EmulationFsVolumeStateDto::Dirty
            } else {
                EmulationFsVolumeStateDto::Clean
            },
            false,
            volume.log_bytes,
        ),
        VolumeAssessment::Unsupported(item) => item,
    }
}

fn repaired_item(volume: PlannedVolume) -> EmulationFsRepairItemDto {
    let repaired = volume.plan.is_some();
    item(
        volume.record.partition_index,
        if repaired {
            EmulationFsVolumeStateDto::Dirty
        } else {
            EmulationFsVolumeStateDto::Clean
        },
        repaired,
        volume.log_bytes,
    )
}

fn repair_result(
    context: &BypassCaseContext<'_>,
    items: Vec<EmulationFsRepairItemDto>,
) -> EmulationFsRepairResultDto {
    EmulationFsRepairResultDto {
        session_id: String::new(),
        data_source_id: context.data_source_id.0.clone(),
        items,
    }
}

fn assess(snapshot: &fs_xfs::log::XfsLogSnapshot) -> fs_xfs::log::XfsLogState {
    fs_xfs::log::assess_log_state(snapshot)
}

fn item(
    partition_index: u32,
    state: EmulationFsVolumeStateDto,
    repaired: bool,
    log_bytes: u64,
) -> EmulationFsRepairItemDto {
    EmulationFsRepairItemDto {
        partition_index,
        state,
        repaired,
        log_bytes,
    }
}

#[cfg(test)]
#[path = "../tests/unit/emulation_fs_repair.rs"]
mod tests;
