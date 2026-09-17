//! Host-side ext4 journal checkpointing for Linux emulation overlays.
//!
//! A captured ext4 filesystem may advertise committed jbd2 transactions even
//! though the guest has not mounted it yet.  The guest would replay those
//! transactions after a host-side shadow edit, potentially restoring the
//! original password file.  This service replays the committed records into
//! the session COW and then checkpoints the journal superblock.  Evidence is
//! never written.

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

const MAX_JOURNAL_BYTES: usize = 256 * 1024 * 1024;
const WRITE_CHUNK_BYTES: usize = 8 * 1024 * 1024;

struct Ext4Volume {
    fs: fs_ext4::Ext4Reader,
    mapping: VolumeMapping,
}

struct PlannedVolume {
    record: DataSourcePartitionRecord,
    volume: Ext4Volume,
    plan: Option<fs_ext4::journal::Ext4JournalRepairPlan>,
    journal_bytes: u64,
}

enum VolumeMapping {
    Direct {
        partition_offset: u64,
        partition_length: u64,
    },
    Lvm {
        extents: Vec<fs_lvm::LvExtent>,
    },
}

impl VolumeMapping {
    fn translate_run(&self, offset: u64) -> Result<(u64, u64), EmulationBypassError> {
        match self {
            Self::Direct {
                partition_offset,
                partition_length,
            } => {
                if offset >= *partition_length {
                    return Err(EmulationBypassError::Edit(
                        "ext4 journal patch starts beyond the partition end".into(),
                    ));
                }
                let absolute = partition_offset
                    .checked_add(offset)
                    .ok_or_else(|| EmulationBypassError::Edit("address overflow".into()))?;
                Ok((absolute, partition_length - offset))
            }
            Self::Lvm { extents } => {
                let index = extents
                    .partition_point(|extent| extent.logical_start <= offset)
                    .checked_sub(1)
                    .ok_or_else(|| EmulationBypassError::Edit("offset below LV map".into()))?;
                let extent = &extents[index];
                let end = extent
                    .logical_start
                    .checked_add(extent.length)
                    .ok_or_else(|| EmulationBypassError::Edit("LV extent overflow".into()))?;
                if offset >= end {
                    return Err(EmulationBypassError::Edit(
                        "ext4 journal patch crosses an LV mapping gap".into(),
                    ));
                }
                let absolute = extent
                    .physical_offset
                    .checked_add(offset - extent.logical_start)
                    .ok_or_else(|| EmulationBypassError::Edit("address overflow".into()))?;
                Ok((absolute, end - offset))
            }
        }
    }
}

/// Assess and repair every ext4 volume belonging to the source.  Planning is
/// completed for all volumes before the first COW write, so an unsupported
/// secondary filesystem cannot leave the session half-repaired.
pub fn repair_ext4_journals(
    disk: &Arc<CowDisk>,
    context: &BypassCaseContext<'_>,
) -> Result<EmulationFsRepairResultDto, EmulationBypassError> {
    DataSourceRepo::new(context.case_conn)
        .find_by_case(context.case_id)?
        .into_iter()
        .find(|candidate| candidate.id == *context.data_source_id)
        .ok_or(EmulationBypassError::PartitionNotFound { partition_index: 0 })?;
    let source_db = crate::source_db::open_ready_source_read_only_by_id(
        context.case_conn,
        context.case_root,
        context.case_id,
        context.data_source_id,
    )
    .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    let records =
        PartitionRepo::new(&source_db.connection).find_by_data_source(&context.data_source_id.0)?;
    let mut planned = Vec::new();
    for record in records
        .into_iter()
        .filter(|record| record.filesystem.as_deref() == Some("Ext4"))
    {
        planned.push(plan_volume(disk, record)?);
    }
    for volume in &planned {
        validate_plan(&volume.volume.mapping, volume.plan.as_ref())?;
    }
    for volume in &planned {
        apply_plan(disk, &volume.volume.mapping, volume.plan.as_ref())?;
    }
    disk.flush()
        .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))?;
    for volume in &planned {
        verify_volume(disk, volume)?;
    }
    let items = planned.into_iter().map(repair_item).collect();
    Ok(EmulationFsRepairResultDto {
        session_id: String::new(),
        data_source_id: context.data_source_id.0.clone(),
        items,
    })
}

fn plan_volume(
    disk: &Arc<CowDisk>,
    record: DataSourcePartitionRecord,
) -> Result<PlannedVolume, EmulationBypassError> {
    let volume = open_volume(disk, &record)?;
    let journal_bytes = volume
        .fs
        .read_internal_journal_superblock()
        .map(|superblock| u64::from(superblock.max_len) * u64::from(superblock.block_size))
        .unwrap_or(0);
    let plan = volume
        .fs
        .plan_journal_repair(MAX_JOURNAL_BYTES)
        .map_err(|error| EmulationBypassError::Unsupported(format!("ext4 journal: {error}")))?;
    Ok(PlannedVolume {
        record,
        volume,
        plan,
        journal_bytes,
    })
}

fn open_volume(
    disk: &Arc<CowDisk>,
    record: &DataSourcePartitionRecord,
) -> Result<Ext4Volume, EmulationBypassError> {
    if record.lvm_lv_name.is_some() {
        let offsets: Vec<u64> = record
            .lvm_pv_offsets_json
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .filter(|values: &Vec<u64>| !values.is_empty())
            .ok_or_else(|| EmulationBypassError::Unsupported("LV has no PV offsets".into()))?;
        let readers = offsets
            .iter()
            .map(|_| Box::new(CowDiskReader::new(Arc::clone(disk))) as Box<dyn EvidenceReader>)
            .collect();
        let pool = fs_lvm::LvmPool::discover(readers, offsets)
            .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
        let index = pool
            .list_volumes()
            .iter()
            .position(|volume| {
                Some(volume.name.as_str()) == record.lvm_lv_name.as_deref()
                    && Some(volume.uuid.as_str()) == record.lvm_lv_uuid.as_deref()
            })
            .ok_or(EmulationBypassError::PartitionNotFound {
                partition_index: record.partition_index,
            })?;
        let lv = pool
            .open_volume(index)
            .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
        let mapping = VolumeMapping::Lvm {
            extents: lv.extent_map().to_vec(),
        };
        let fs = fs_ext4::Ext4Reader::open(Box::new(lv), 0)
            .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
        Ok(Ext4Volume { fs, mapping })
    } else {
        if record.length == 0 {
            return Err(EmulationBypassError::Unsupported(
                "ext4 partition has no declared length".into(),
            ));
        }
        let reader: Box<dyn EvidenceReader> = Box::new(CowDiskReader::new(Arc::clone(disk)));
        let window = PartitionWindowReader::new(reader, record.offset, Some(record.length))
            .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
        let fs = fs_ext4::Ext4Reader::open(Box::new(window), 0)
            .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
        Ok(Ext4Volume {
            fs,
            mapping: VolumeMapping::Direct {
                partition_offset: record.offset,
                partition_length: record.length,
            },
        })
    }
}

fn validate_plan(
    mapping: &VolumeMapping,
    plan: Option<&fs_ext4::journal::Ext4JournalRepairPlan>,
) -> Result<(), EmulationBypassError> {
    let Some(plan) = plan else {
        return Ok(());
    };
    for patch in &plan.patches {
        let mut offset = 0usize;
        while offset < patch.bytes.len() {
            let (_, run) = mapping.translate_run(patch.volume_offset + offset as u64)?;
            let chunk = usize::try_from(run)
                .unwrap_or(usize::MAX)
                .min(patch.bytes.len() - offset);
            if chunk == 0 {
                return Err(EmulationBypassError::Edit(
                    "empty ext4 journal patch run".into(),
                ));
            }
            offset += chunk;
        }
    }
    Ok(())
}

fn apply_plan(
    disk: &Arc<CowDisk>,
    mapping: &VolumeMapping,
    plan: Option<&fs_ext4::journal::Ext4JournalRepairPlan>,
) -> Result<(), EmulationBypassError> {
    let Some(plan) = plan else {
        return Ok(());
    };
    for patch in &plan.patches {
        let mut offset = 0usize;
        while offset < patch.bytes.len() {
            let (absolute, run) = mapping.translate_run(patch.volume_offset + offset as u64)?;
            let chunk = usize::try_from(run)
                .unwrap_or(usize::MAX)
                .min(patch.bytes.len() - offset)
                .min(WRITE_CHUNK_BYTES);
            if chunk == 0 {
                return Err(EmulationBypassError::Edit(
                    "empty ext4 journal write run".into(),
                ));
            }
            disk.write_all_at(absolute, &patch.bytes[offset..offset + chunk])
                .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))?;
            offset += chunk;
        }
    }
    Ok(())
}

fn verify_volume(disk: &Arc<CowDisk>, planned: &PlannedVolume) -> Result<(), EmulationBypassError> {
    if planned.plan.is_none() {
        return Ok(());
    }
    let verified = open_volume(disk, &planned.record)?;
    verified
        .fs
        .verify_journal_repair()
        .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))
}

fn repair_item(planned: PlannedVolume) -> EmulationFsRepairItemDto {
    let repaired = planned.plan.is_some();
    EmulationFsRepairItemDto {
        partition_index: planned.record.partition_index,
        initial_state: if repaired {
            EmulationFsVolumeStateDto::Dirty
        } else {
            EmulationFsVolumeStateDto::Clean
        },
        state: EmulationFsVolumeStateDto::Clean,
        repaired,
        log_bytes: planned.journal_bytes,
    }
}
