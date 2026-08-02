use std::path::PathBuf;

use domain::{DataSourceId, DataSourceKind, DataSourcePlatform};
use evidence_core::EvidenceReader;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo,
    partition_repo::{DataSourcePartitionRecord, PartitionRepo},
};
use rusqlite::Connection;

use crate::datasource_service::{
    ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemSource, LvmLogicalVolumeIdentity,
    LvmPhysicalVolumeSource,
};

use super::{DeletedRecoveryContext, DeletedRecoveryError};

#[derive(Debug, Clone)]
pub(super) struct RecoverySource {
    pub path: PathBuf,
    pub kind: DataSourceKind,
}

#[derive(Debug, Clone)]
pub(super) struct RecoveryTarget {
    pub partition: DataSourcePartitionRecord,
    pub candidate: ImageFilesystemCandidate,
    pub filesystem_type: &'static str,
}

pub(super) fn load_source(
    case_conn: &Connection,
    data_source_id: &DataSourceId,
) -> Result<RecoverySource, DeletedRecoveryError> {
    let repo = DataSourceRepo::new(case_conn);
    Ok(RecoverySource {
        path: PathBuf::from(repo.source_path(data_source_id)?),
        kind: repo.source_kind(data_source_id)?,
    })
}

pub(super) fn load_targets(
    source_conn: &Connection,
    data_source_id: &DataSourceId,
    platform: DataSourcePlatform,
    requested_partition: Option<u32>,
) -> Result<Vec<RecoveryTarget>, DeletedRecoveryError> {
    let partitions = PartitionRepo::new(source_conn).find_by_data_source(&data_source_id.0)?;
    let mut targets = Vec::new();
    for partition in partitions {
        if requested_partition.is_some_and(|requested| requested != partition.partition_index) {
            continue;
        }
        if !is_readable_status(&partition.status) {
            continue;
        }
        let Some((filesystem_type, kind)) = recovery_filesystem(&partition, platform) else {
            continue;
        };
        let candidate = candidate_from_partition(&partition, kind)?;
        targets.push(RecoveryTarget {
            partition,
            candidate,
            filesystem_type,
        });
    }
    targets.sort_by_key(|target| target.partition.partition_index);
    if targets.is_empty() {
        return Err(DeletedRecoveryError::Unsupported(
            match requested_partition {
                Some(index) => format!(
                    "partition {index} is not a ready filesystem supported for {platform} deleted recovery"
                ),
                None => format!(
                    "the {platform} data source has no ready filesystem supported for deleted recovery"
                ),
            },
        ));
    }
    Ok(targets)
}

pub(super) fn open_target_reader(
    context: &DeletedRecoveryContext<'_>,
    source: &RecoverySource,
    target: &RecoveryTarget,
) -> Result<(Box<dyn EvidenceReader>, u64), DeletedRecoveryError> {
    if source.kind == DataSourceKind::CephRbd {
        let reader = crate::ceph_reconstruction::open_derived_rbd_reader(
            context.case_conn,
            context.case_root,
            context.case_id,
            context.data_source_id,
        )
        .map_err(|error| DeletedRecoveryError::Unsupported(error.to_string()))?;
        return Ok((Box::new(reader), target.partition.offset));
    }
    let (reader, offset) = crate::import_pipeline::partition::open_candidate_reader(
        &source.path,
        &source.kind,
        &target.candidate,
    )
    .map_err(DeletedRecoveryError::Parser)?;
    if !crate::partition_capabilities::is_bitlocker_partition(&target.partition) {
        return Ok((reader, offset));
    }
    let runtime = context
        .bitlocker_runtime
        .as_ref()
        .ok_or_else(|| DeletedRecoveryError::BitLockerLocked)?;
    let length = (target.partition.length > 0).then_some(target.partition.length);
    crate::bitlocker_runtime::open_registered_bitlocker_volume(
        reader,
        offset,
        length,
        &context.case_id.0,
        &context.data_source_id.0,
        target.partition.partition_index as usize,
        runtime,
    )
    .map(|reader| (reader, 0))
    .map_err(map_bitlocker_runtime_error)
}

fn map_bitlocker_runtime_error(
    error: crate::bitlocker_runtime::BitLockerRuntimeError,
) -> DeletedRecoveryError {
    match error {
        crate::bitlocker_runtime::BitLockerRuntimeError::Locked => {
            DeletedRecoveryError::BitLockerLocked
        }
        crate::bitlocker_runtime::BitLockerRuntimeError::RegistryUnavailable => {
            DeletedRecoveryError::InvalidState(
                "BitLocker runtime registry is unavailable".to_string(),
            )
        }
        crate::bitlocker_runtime::BitLockerRuntimeError::InvalidWindow(error) => {
            DeletedRecoveryError::Io(error)
        }
        crate::bitlocker_runtime::BitLockerRuntimeError::Volume(error) => {
            DeletedRecoveryError::Parser(format!("BitLocker volume open failed: {error}"))
        }
    }
}

fn recovery_filesystem(
    partition: &DataSourcePartitionRecord,
    platform: DataSourcePlatform,
) -> Option<(&'static str, ImageFilesystemKind)> {
    let label = partition
        .filesystem
        .as_deref()
        .unwrap_or(&partition.kind_label)
        .trim()
        .to_ascii_lowercase();
    if platform == DataSourcePlatform::Windows && (label == "ntfs" || label.contains("ntfs")) {
        return Some(("ntfs", ImageFilesystemKind::Ntfs));
    }
    if platform == DataSourcePlatform::Linux && (label == "ext4" || label.contains("ext4")) {
        return Some(("ext4", ImageFilesystemKind::Ext4));
    }
    if platform == DataSourcePlatform::Linux && (label == "xfs" || label.contains("xfs")) {
        return Some(("xfs", ImageFilesystemKind::Xfs));
    }
    None
}

fn candidate_from_partition(
    partition: &DataSourcePartitionRecord,
    kind: ImageFilesystemKind,
) -> Result<ImageFilesystemCandidate, DeletedRecoveryError> {
    let lvm_identity = lvm_identity(partition)?;
    Ok(ImageFilesystemCandidate {
        partition_index: Some(partition.partition_index as usize),
        partition_name: Some(partition.name.clone()),
        kind,
        offset: partition.offset,
        length: Some(partition.length),
        source: if lvm_identity.is_some() {
            ImageFilesystemSource::LvmLogicalVolume
        } else {
            ImageFilesystemSource::DirectVolume
        },
        lvm_identity,
    })
}

fn lvm_identity(
    partition: &DataSourcePartitionRecord,
) -> Result<Option<LvmLogicalVolumeIdentity>, DeletedRecoveryError> {
    let Some(offsets_json) = partition.lvm_pv_offsets_json.as_deref() else {
        return Ok(None);
    };
    let pv_offsets = serde_json::from_str::<Vec<u64>>(offsets_json).map_err(|error| {
        DeletedRecoveryError::InvalidState(format!("invalid LVM offsets: {error}"))
    })?;
    if pv_offsets.is_empty() {
        return Ok(None);
    }
    let pv_sources = partition
        .lvm_pv_sources_json
        .as_deref()
        .map(serde_json::from_str::<Vec<LvmPhysicalVolumeSource>>)
        .transpose()
        .map_err(|error| {
            DeletedRecoveryError::InvalidState(format!("invalid LVM source routing: {error}"))
        })?
        .unwrap_or_default();
    let lv_uuid = partition.lvm_lv_uuid.clone().unwrap_or_default();
    let lv_name = partition.lvm_lv_name.clone().unwrap_or_default();
    if lv_uuid.is_empty() && lv_name.is_empty() {
        return Err(DeletedRecoveryError::InvalidState(
            "LVM partition is missing logical-volume identity".to_string(),
        ));
    }
    Ok(Some(LvmLogicalVolumeIdentity {
        vg_uuid: partition.lvm_vg_uuid.clone().unwrap_or_default(),
        vg_name: partition.lvm_vg_name.clone().unwrap_or_default(),
        lv_uuid,
        lv_name,
        pv_offsets,
        pv_sources,
    }))
}

fn is_readable_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "supported" | "queued" | "done" | "ready"
    )
}

#[cfg(test)]
#[path = "../../tests/unit/deleted_recovery/source.rs"]
mod tests;
