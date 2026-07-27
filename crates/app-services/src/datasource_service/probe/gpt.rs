use super::{partition_display_name, partition_status_for_filesystem};
use crate::datasource_service::fs_magic::{
    kind_label, read_boot_filesystem, read_sector, SECTOR_SIZE,
};
use crate::datasource_service::{
    ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemProbe, ImageFilesystemSource,
    PartitionRecord, PartitionStatus, Result,
};
use evidence_core::volume::gpt::GptPartition;
use std::io::{Read, Seek, SeekFrom};

struct GptPartitionProbe {
    candidate: Option<ImageFilesystemCandidate>,
    record: PartitionRecord,
    warning: Option<String>,
}

pub(super) fn detect_gpt_filesystems<R>(reader: &mut R) -> Result<ImageFilesystemProbe>
where
    R: Read + Seek,
{
    let partitions = read_gpt_partitions(reader)?;
    let mut candidates = Vec::new();
    let mut records = Vec::new();
    let mut warnings = Vec::new();

    for partition in partitions
        .iter()
        .filter(|partition| partition.start_lba > 0)
    {
        let probe = probe_gpt_partition(reader, partition)?;
        if let Some(candidate) = probe.candidate {
            candidates.push(candidate);
        }
        if let Some(warning) = probe.warning {
            warnings.push(warning);
        }
        records.push(probe.record);
    }

    Ok(ImageFilesystemProbe {
        candidates,
        partitions: records,
        unsupported_volumes: Vec::new(),
        warnings,
    })
}

fn read_gpt_partitions<R>(reader: &mut R) -> Result<Vec<GptPartition>>
where
    R: Read + Seek,
{
    let header_sector = read_sector(reader, SECTOR_SIZE)?;
    let Some(header) = evidence_core::volume::gpt::parse_gpt_header(&header_sector) else {
        return Ok(Vec::new());
    };
    let entry_bytes = header.entry_size.saturating_mul(header.partition_count);
    if entry_bytes == 0 {
        return Ok(Vec::new());
    }

    reader.seek(SeekFrom::Start(header.partition_entry_lba * SECTOR_SIZE))?;
    let mut entry_data = vec![0u8; entry_bytes as usize];
    reader.read_exact(&mut entry_data)?;
    Ok(evidence_core::volume::gpt::parse_gpt_entries(
        &entry_data,
        header.entry_size,
        header.partition_count,
    ))
}

fn probe_gpt_partition<R>(reader: &mut R, partition: &GptPartition) -> Result<GptPartitionProbe>
where
    R: Read + Seek,
{
    let offset = partition.start_lba * SECTOR_SIZE;
    let partition_type = evidence_core::volume::gpt::classify_partition_type(&partition.type_guid);
    let type_name = evidence_core::volume::gpt::partition_type_name(partition_type);
    let fs_kind = read_boot_filesystem(reader, offset)?;
    let kind_label = fs_kind
        .map(kind_label)
        .unwrap_or_else(|| type_name.to_string());
    let status = fs_kind
        .map(partition_status_for_filesystem)
        .unwrap_or(PartitionStatus::Unsupported);
    let display_name = partition_display_name(
        partition.index,
        &kind_label,
        Some(&partition.name),
        Some(type_name),
    );

    Ok(GptPartitionProbe {
        candidate: gpt_candidate(partition, fs_kind, offset),
        warning: gpt_partition_warning(partition, &display_name, type_name, status),
        record: PartitionRecord {
            index: partition.index,
            name: display_name,
            kind_label,
            type_guid: Some(evidence_core::volume::gpt::format_guid(
                &partition.type_guid,
            )),
            offset,
            length: partition
                .end_lba
                .saturating_sub(partition.start_lba)
                .saturating_add(1)
                * SECTOR_SIZE,
            status,
            filesystem: fs_kind,
            lvm_identity: None,
        },
    })
}

fn gpt_candidate(
    partition: &GptPartition,
    fs_kind: Option<ImageFilesystemKind>,
    offset: u64,
) -> Option<ImageFilesystemCandidate> {
    let kind = match fs_kind {
        Some(
            kind @ (ImageFilesystemKind::Ntfs
            | ImageFilesystemKind::Fat
            | ImageFilesystemKind::Ext4
            | ImageFilesystemKind::Xfs
            | ImageFilesystemKind::Btrfs
            | ImageFilesystemKind::LvmPool
            | ImageFilesystemKind::BitLocker),
        ) => kind,
        None => return None,
    };
    Some(ImageFilesystemCandidate {
        partition_index: Some(partition.index),
        partition_name: Some(partition.name.clone()),
        kind,
        offset,
        length: Some(
            partition
                .end_lba
                .saturating_sub(partition.start_lba)
                .saturating_add(1)
                * SECTOR_SIZE,
        ),
        source: ImageFilesystemSource::GptPartition,
        lvm_identity: None,
    })
}

fn gpt_partition_warning(
    partition: &GptPartition,
    display_name: &str,
    type_name: &str,
    status: PartitionStatus,
) -> Option<String> {
    match status {
        PartitionStatus::EncryptedBitLocker => Some(format!(
            "Partition {} '{}' is BitLocker-encrypted and currently locked",
            partition.index, display_name
        )),
        PartitionStatus::Unsupported => Some(format!(
            "Partition {} '{}' is not yet supported ({}, GUID {})",
            partition.index,
            display_name,
            type_name,
            evidence_core::volume::gpt::format_guid(&partition.type_guid)
        )),
        PartitionStatus::Supported | PartitionStatus::Expanded => None,
    }
}
