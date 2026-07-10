use super::fs_magic::{kind_label, read_boot_filesystem, read_sector, SECTOR_SIZE};
use super::{
    DataSourceError, ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemProbe,
    ImageFilesystemSource, PartitionRecord, PartitionStatus, Result,
};
use std::io::{Read, Seek, SeekFrom};

/// Build an honest partition display name from on-disk metadata.
///
/// NTFS/FAT boot sectors do not carry Windows drive-letter assignments. A real
/// `C:`-style letter requires matching the offline SYSTEM MountedDevices hive,
/// which this probe path does not currently parse.
pub fn partition_display_name(
    index: usize,
    kind_label: &str,
    candidate_name: Option<&str>,
    partition_type_name: Option<&str>,
) -> String {
    let partition_label = format!("Partition {index}");
    let kind_label = kind_label.trim();
    let base_name = if kind_label.is_empty() {
        partition_label.clone()
    } else {
        format!("{partition_label} ({kind_label})")
    };

    match candidate_name.and_then(|name| meaningful_partition_name(name, partition_type_name)) {
        Some(name) => format!("{base_name} - {name}"),
        None => base_name,
    }
}

pub fn volume_display_name(kind_label: &str, candidate_name: Option<&str>) -> String {
    let kind_label = kind_label.trim();
    let base_name = if kind_label.is_empty() {
        "Volume".to_string()
    } else {
        format!("Volume ({kind_label})")
    };

    match candidate_name.and_then(|name| meaningful_partition_name(name, None)) {
        Some(name) => format!("{base_name} - {name}"),
        None => base_name,
    }
}

pub fn detect_image_filesystem<R>(reader: &mut R) -> Result<ImageFilesystemProbe>
where
    R: Read + Seek,
{
    let mut warnings = Vec::new();
    let mut candidates = Vec::new();
    let mut partitions = Vec::new();

    if let Some(kind) = read_boot_filesystem(reader, 0)? {
        candidates.push(ImageFilesystemCandidate {
            partition_index: Some(1),
            partition_name: Some("Volume".to_string()),
            kind,
            offset: 0,
            source: ImageFilesystemSource::DirectVolume,
            lvm_identity: None,
        });
        return Ok(ImageFilesystemProbe {
            candidates,
            partitions: vec![PartitionRecord {
                index: 1,
                name: "Volume".to_string(),
                kind_label: kind_label(kind),
                type_guid: None,
                offset: 0,
                length: 0,
                status: match kind {
                    ImageFilesystemKind::Ntfs | ImageFilesystemKind::Fat => {
                        PartitionStatus::Supported
                    }
                    ImageFilesystemKind::BitLocker => PartitionStatus::EncryptedBitLocker,
                    ImageFilesystemKind::Ext4
                    | ImageFilesystemKind::Xfs
                    | ImageFilesystemKind::Btrfs
                    | ImageFilesystemKind::LvmPool => PartitionStatus::Supported,
                },
                filesystem: Some(kind),
                lvm_identity: None,
            }],
            warnings,
        });
    }

    let mbr_entries = evidence_core::volume::mbr::parse_mbr_full(reader)
        .map_err(|e| DataSourceError::Evidence(format!("MBR read error: {}", e)))?;
    let mbr_types: Vec<String> = mbr_entries
        .iter()
        .filter(|entry| entry.partition_type != 0)
        .map(|entry| format!("{:02X}", entry.partition_type))
        .collect();

    let is_gpt_protective = mbr_entries.iter().any(|entry| entry.partition_type == 0xEE);

    // Push filesystem candidates from primary + logical partitions.
    for entry in &mbr_entries {
        if entry.is_extended() || entry.lba_start == 0 {
            continue;
        }
        let offset = entry.lba_start as u64 * SECTOR_SIZE;
        let name = if entry.is_logical {
            Some(format!("Logical Volume {}", entry.partition_number))
        } else {
            Some(format!("Partition {}", entry.partition_number))
        };
        if let Some(kind) = read_boot_filesystem(reader, offset)? {
            push_candidate(
                &mut candidates,
                Some(entry.partition_number),
                name,
                kind,
                offset,
                ImageFilesystemSource::MbrPartition,
            );
        }
    }

    // Build PartitionRecord entries for non-empty, non-extended MBR partitions
    // only when not falling through to GPT, which produces its own records.
    if !is_gpt_protective {
        for entry in &mbr_entries {
            if entry.is_extended() || entry.partition_type == 0 {
                continue;
            }
            let offset = entry.lba_start as u64 * SECTOR_SIZE;
            let length = entry.sector_count as u64 * SECTOR_SIZE;
            let class =
                evidence_core::volume::mbr::classify_mbr_partition_type(entry.partition_type);
            let fs_kind = read_boot_filesystem(reader, offset)?;
            let kind_label = fs_kind
                .map(kind_label)
                .unwrap_or_else(|| class.name.to_string());
            let status = if let Some(kind) = fs_kind {
                match kind {
                    ImageFilesystemKind::Ntfs | ImageFilesystemKind::Fat => {
                        PartitionStatus::Supported
                    }
                    ImageFilesystemKind::BitLocker => PartitionStatus::EncryptedBitLocker,
                    ImageFilesystemKind::Ext4
                    | ImageFilesystemKind::Xfs
                    | ImageFilesystemKind::Btrfs
                    | ImageFilesystemKind::LvmPool => PartitionStatus::Supported,
                }
            } else {
                match class.status {
                    evidence_core::volume::mbr::MbrPartitionStatus::Supported => {
                        PartitionStatus::Supported
                    }
                    evidence_core::volume::mbr::MbrPartitionStatus::EncryptedBitLocker => {
                        PartitionStatus::EncryptedBitLocker
                    }
                    evidence_core::volume::mbr::MbrPartitionStatus::Unsupported => {
                        PartitionStatus::Unsupported
                    }
                }
            };
            let display_name =
                partition_display_name(entry.partition_number, &kind_label, None, Some(class.name));

            if status == PartitionStatus::EncryptedBitLocker {
                warnings.push(format!(
                    "Partition {} '{}' is BitLocker-encrypted",
                    entry.partition_number, display_name,
                ));
            } else if status == PartitionStatus::Unsupported {
                warnings.push(format!(
                    "Partition {} '{}' is not yet supported (type 0x{:02X})",
                    entry.partition_number, display_name, entry.partition_type,
                ));
            } else if matches!(fs_kind, Some(ImageFilesystemKind::LvmPool)) {
                // LVM pool detected; discovery/expansion happens in import pipeline.
                tracing::info!(
                    "LVM2 physical volume detected at partition {} ({}), LV expansion deferred to import",
                    entry.partition_number, display_name,
                );
            }

            partitions.push(PartitionRecord {
                index: entry.partition_number,
                name: display_name,
                kind_label,
                type_guid: None,
                offset,
                length,
                status,
                filesystem: fs_kind,
                lvm_identity: None,
            });
        }
    }

    if is_gpt_protective {
        let gpt_probe = detect_gpt_filesystems(reader)?;
        for candidate in gpt_probe.candidates {
            push_candidate(
                &mut candidates,
                candidate.partition_index,
                candidate.partition_name.clone(),
                candidate.kind,
                candidate.offset,
                ImageFilesystemSource::GptPartition,
            );
        }
        partitions = gpt_probe.partitions;
        warnings.extend(gpt_probe.warnings);
    }

    if !candidates.is_empty() {
        return Ok(ImageFilesystemProbe {
            candidates,
            partitions,
            warnings,
        });
    }

    if partitions.is_empty() {
        warnings.push(format!(
            "No supported NTFS/FAT filesystem detected. MBR types: [{}]",
            mbr_types.join(", ")
        ));
    }
    Ok(ImageFilesystemProbe {
        candidates,
        partitions,
        warnings,
    })
}

fn push_candidate(
    candidates: &mut Vec<ImageFilesystemCandidate>,
    partition_index: Option<usize>,
    partition_name: Option<String>,
    kind: ImageFilesystemKind,
    offset: u64,
    source: ImageFilesystemSource,
) {
    if candidates
        .iter()
        .any(|candidate| candidate.offset == offset && candidate.kind == kind)
    {
        return;
    }

    candidates.push(ImageFilesystemCandidate {
        partition_index,
        partition_name,
        kind,
        offset,
        source,
        lvm_identity: None,
    });
}

fn detect_gpt_filesystems<R>(reader: &mut R) -> Result<ImageFilesystemProbe>
where
    R: Read + Seek,
{
    let header_sector = read_sector(reader, SECTOR_SIZE)?;
    let Some(header) = evidence_core::volume::gpt::parse_gpt_header(&header_sector) else {
        return Ok(ImageFilesystemProbe {
            candidates: Vec::new(),
            partitions: Vec::new(),
            warnings: Vec::new(),
        });
    };

    let entry_bytes = header.entry_size.saturating_mul(header.partition_count);
    if entry_bytes == 0 {
        return Ok(ImageFilesystemProbe {
            candidates: Vec::new(),
            partitions: Vec::new(),
            warnings: Vec::new(),
        });
    }

    reader.seek(SeekFrom::Start(header.partition_entry_lba * SECTOR_SIZE))?;
    let mut entry_data = vec![0u8; entry_bytes as usize];
    reader.read_exact(&mut entry_data)?;
    let partitions = evidence_core::volume::gpt::parse_gpt_entries(
        &entry_data,
        header.entry_size,
        header.partition_count,
    );

    let mut candidates = Vec::new();
    let mut records = Vec::new();
    let mut warnings = Vec::new();

    for partition in partitions
        .iter()
        .filter(|partition| partition.start_lba > 0)
    {
        let offset = partition.start_lba * SECTOR_SIZE;
        let length = partition
            .end_lba
            .saturating_sub(partition.start_lba)
            .saturating_add(1)
            * SECTOR_SIZE;
        let partition_type =
            evidence_core::volume::gpt::classify_partition_type(&partition.type_guid);
        let type_name = evidence_core::volume::gpt::partition_type_name(partition_type);
        let fs_kind = read_boot_filesystem(reader, offset)?;
        let kind_label = fs_kind
            .map(kind_label)
            .unwrap_or_else(|| type_name.to_string());
        let mut status = PartitionStatus::Unsupported;

        if let Some(kind) = fs_kind {
            status = match kind {
                ImageFilesystemKind::Ntfs | ImageFilesystemKind::Fat => PartitionStatus::Supported,
                ImageFilesystemKind::BitLocker => PartitionStatus::EncryptedBitLocker,
                ImageFilesystemKind::Ext4
                | ImageFilesystemKind::Xfs
                | ImageFilesystemKind::Btrfs
                | ImageFilesystemKind::LvmPool => PartitionStatus::Supported,
            };
        }

        if let Some(kind) = fs_kind {
            if matches!(
                kind,
                ImageFilesystemKind::Ntfs
                    | ImageFilesystemKind::Fat
                    | ImageFilesystemKind::Ext4
                    | ImageFilesystemKind::Xfs
                    | ImageFilesystemKind::Btrfs
                    | ImageFilesystemKind::LvmPool
            ) {
                candidates.push(ImageFilesystemCandidate {
                    partition_index: Some(partition.index),
                    partition_name: Some(partition.name.clone()),
                    kind,
                    offset,
                    source: ImageFilesystemSource::GptPartition,
                    lvm_identity: None,
                });
            }
        }

        if status == PartitionStatus::EncryptedBitLocker {
            let display_name = partition_display_name(
                partition.index,
                &kind_label,
                Some(&partition.name),
                Some(type_name),
            );
            warnings.push(format!(
                "Partition {} '{}' is BitLocker-encrypted and currently locked",
                partition.index, display_name
            ));
        } else if status == PartitionStatus::Unsupported {
            let display_name = partition_display_name(
                partition.index,
                &kind_label,
                Some(&partition.name),
                Some(type_name),
            );
            warnings.push(format!(
                "Partition {} '{}' is not yet supported ({}, GUID {})",
                partition.index,
                display_name,
                type_name,
                evidence_core::volume::gpt::format_guid(&partition.type_guid)
            ));
        }

        let display_name = partition_display_name(
            partition.index,
            &kind_label,
            Some(&partition.name),
            Some(type_name),
        );

        records.push(PartitionRecord {
            index: partition.index,
            name: display_name,
            kind_label,
            type_guid: Some(evidence_core::volume::gpt::format_guid(
                &partition.type_guid,
            )),
            offset,
            length,
            status,
            filesystem: fs_kind,
            lvm_identity: None,
        });
    }

    Ok(ImageFilesystemProbe {
        candidates,
        partitions: records,
        warnings,
    })
}

fn meaningful_partition_name<'a>(
    name: &'a str,
    partition_type_name: Option<&str>,
) -> Option<&'a str> {
    let trimmed = name.trim();
    if trimmed.is_empty() || is_misleading_partition_name(trimmed) {
        return None;
    }

    if partition_type_name.is_some_and(|type_name| trimmed.eq_ignore_ascii_case(type_name.trim())) {
        return None;
    }

    Some(trimmed)
}

fn is_misleading_partition_name(name: &str) -> bool {
    let normalized = name.trim().trim_matches('\0');
    if normalized.is_empty() {
        return true;
    }

    let slash_trimmed = normalized.trim_matches(|ch| ch == '/' || ch == '\\');
    if slash_trimmed.is_empty() || matches!(normalized, "." | "..") {
        return true;
    }

    let collapsed = normalized
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    matches!(
        collapsed.as_str(),
        "system volume information"
            | "$mft"
            | "root"
            | "volume"
            | "ntfs"
            | "fat"
            | "fat32"
            | "microsoft basic data"
            | "basic data"
            | "basic data partition"
            | "efi system"
            | "microsoft reserved"
            | "windows recovery"
            | "unknown"
    )
}
