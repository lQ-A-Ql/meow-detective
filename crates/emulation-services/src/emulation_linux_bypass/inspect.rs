//! Read-only pre-flight inspection of one partition's bypass options.
//!
//! Gathers volume reflink capability, target-inode reflink flag, log state,
//! blockers and a recommended method without opening a session overlay,
//! writing evidence, or invalidating the disk. The desktop layer overrides
//! `appliance_available` and fills `session_id`.

use transport::dto::{EmulationLinuxBypassBlockerDto, EmulationLinuxBypassInspectionDto};

use super::common::SHADOW_PATH;
use super::plan::{blocker_alternatives, ext4_journal_requires_repair, recommended_method};
use super::volume::{
    load_partition_record, open_linux_partition, record_encrypted, record_filesystem_kind,
    xfs_blocker_to_dto, FilesystemKind, LinuxFilesystem, LinuxPartition,
};
use crate::emulation_bypass::{BypassCaseContext, EmulationBypassError};

/// Read-only assessment returned before any edit is attempted.
pub fn inspect_linux_bypass(
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
) -> Result<EmulationLinuxBypassInspectionDto, EmulationBypassError> {
    let (_, record) = load_partition_record(case_context, partition_index)?;
    let encrypted = record_encrypted(&record);
    let kind = record_filesystem_kind(&record);
    let filesystem_label = record.filesystem.clone().unwrap_or_default();

    let mut volume_reflink_enabled = false;
    let mut target_inode_reflink = None;
    let mut log_clean = false;
    let mut blockers: Vec<EmulationLinuxBypassBlockerDto> = Vec::new();
    let mut supported = false;

    match kind {
        Some(FilesystemKind::Ext4) => {
            let partition = open_linux_partition(case_context, partition_index, None)?;
            let LinuxFilesystem::Ext4(fs) = &partition.fs else {
                unreachable!("ext4 record opens an Ext4 reader");
            };
            log_clean = !ext4_journal_requires_repair(fs)?;
            supported = log_clean;
            if !log_clean {
                blockers.push(EmulationLinuxBypassBlockerDto::DirtyLog);
            }
        }
        Some(FilesystemKind::Xfs) => {
            let partition = open_linux_partition(case_context, partition_index, None)?;
            let LinuxFilesystem::Xfs(fs) = &partition.fs else {
                unreachable!("xfs record opens an XFS reader");
            };
            let shadow_bytes = current_shadow_bytes(&partition)?;
            let assessment = fs.assess_in_place_file_rewrite(SHADOW_PATH, &shadow_bytes);
            volume_reflink_enabled = assessment.volume_reflink_enabled;
            target_inode_reflink = assessment.target_inode_reflink;
            log_clean = assessment.log_clean;
            supported = assessment.supported;
            if let Some(blocker) = assessment.blocker.map(xfs_blocker_to_dto) {
                blockers.push(blocker);
            }
        }
        None if encrypted => blockers.push(EmulationLinuxBypassBlockerDto::EncryptedVolume),
        None => blockers.push(EmulationLinuxBypassBlockerDto::UnsupportedLayout),
    }

    let recommended = recommended_method(supported, &blockers);
    let alternatives = blockers
        .first()
        .map(|blocker| blocker_alternatives(*blocker))
        .unwrap_or_default();

    Ok(EmulationLinuxBypassInspectionDto {
        session_id: String::new(),
        data_source_id: case_context.data_source_id.0.clone(),
        partition_index,
        filesystem: filesystem_label,
        volume_reflink_enabled,
        target_inode_reflink,
        log_clean,
        recommended_method: recommended,
        blockers,
        alternatives,
        appliance_available: false,
    })
}

fn current_shadow_bytes(partition: &LinuxPartition) -> Result<Vec<u8>, EmulationBypassError> {
    let size = partition
        .fs
        .file_size(SHADOW_PATH)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    let length = usize::try_from(size)
        .map_err(|_| EmulationBypassError::Unsupported("shadow file is too large".into()))?;
    partition
        .fs
        .read_file_range(SHADOW_PATH, 0, length)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))
}
