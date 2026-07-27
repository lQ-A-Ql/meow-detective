mod gpt;

use self::gpt::detect_gpt_filesystems;
use super::fs_magic::{kind_label, read_boot_filesystem, SECTOR_SIZE};
use super::{
    DataSourceError, ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemProbe,
    ImageFilesystemSource, PartitionRecord, PartitionStatus, Result, UnsupportedImageKind,
    UnsupportedImageVolume,
};
use evidence_core::volume::mbr::{MbrPartitionStatus, PartitionEntry};
use std::io::{Read, Seek};

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
    if let Some(probe) = detect_direct_volume(reader)? {
        return Ok(probe);
    }

    let mbr_entries = evidence_core::volume::mbr::parse_mbr_full(reader)
        .map_err(|e| DataSourceError::Evidence(format!("MBR read error: {}", e)))?;
    let mbr_types: Vec<String> = mbr_entries
        .iter()
        .filter(|entry| entry.partition_type != 0)
        .map(|entry| format!("{:02X}", entry.partition_type))
        .collect();

    let is_gpt_protective = mbr_entries.iter().any(|entry| entry.partition_type == 0xEE);
    let mut candidates = Vec::new();
    detect_mbr_candidates(reader, &mbr_entries, &mut candidates)?;

    let (mut partitions, mut warnings) = if is_gpt_protective {
        (Vec::new(), Vec::new())
    } else {
        detect_mbr_partitions(reader, &mbr_entries)?
    };

    if is_gpt_protective {
        let gpt_probe = detect_gpt_filesystems(reader)?;
        for candidate in gpt_probe.candidates {
            push_candidate(
                &mut candidates,
                candidate.partition_index,
                candidate.partition_name.clone(),
                candidate.kind,
                candidate.offset,
                candidate.length,
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
            unsupported_volumes: Vec::new(),
            warnings,
        });
    }

    let unsupported_volumes = detect_direct_unsupported_volumes(reader)?;
    if partitions.is_empty() {
        warnings.push(format!(
            "No supported NTFS/FAT filesystem detected. MBR types: [{}]",
            mbr_types.join(", ")
        ));
    }
    Ok(ImageFilesystemProbe {
        candidates,
        partitions,
        unsupported_volumes,
        warnings,
    })
}

fn detect_direct_volume<R>(reader: &mut R) -> Result<Option<ImageFilesystemProbe>>
where
    R: Read + Seek,
{
    let Some(kind) = read_boot_filesystem(reader, 0)? else {
        return Ok(None);
    };
    let candidate = ImageFilesystemCandidate {
        partition_index: Some(1),
        partition_name: Some("Volume".to_string()),
        kind,
        offset: 0,
        length: None,
        source: ImageFilesystemSource::DirectVolume,
        lvm_identity: None,
    };
    let partition = PartitionRecord {
        index: 1,
        name: "Volume".to_string(),
        kind_label: kind_label(kind),
        type_guid: None,
        offset: 0,
        length: 0,
        status: partition_status_for_filesystem(kind),
        filesystem: Some(kind),
        lvm_identity: None,
    };
    Ok(Some(ImageFilesystemProbe {
        candidates: vec![candidate],
        partitions: vec![partition],
        unsupported_volumes: Vec::new(),
        warnings: Vec::new(),
    }))
}

fn detect_direct_unsupported_volumes<R>(reader: &mut R) -> Result<Vec<UnsupportedImageVolume>>
where
    R: Read + Seek,
{
    if super::has_bluestore_label(reader)? {
        return Ok(vec![UnsupportedImageVolume {
            kind: UnsupportedImageKind::CephBlueStore,
            source: ImageFilesystemSource::DirectVolume,
            name: Some("Ceph BlueStore OSD".to_string()),
            size_bytes: None,
            lvm_identity: None,
        }]);
    }
    Ok(Vec::new())
}

fn detect_mbr_candidates<R>(
    reader: &mut R,
    entries: &[PartitionEntry],
    candidates: &mut Vec<ImageFilesystemCandidate>,
) -> Result<()>
where
    R: Read + Seek,
{
    for entry in entries {
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
                candidates,
                Some(entry.partition_number),
                name,
                kind,
                offset,
                Some(entry.sector_count as u64 * SECTOR_SIZE),
                ImageFilesystemSource::MbrPartition,
            );
        }
    }
    Ok(())
}

fn detect_mbr_partitions<R>(
    reader: &mut R,
    entries: &[PartitionEntry],
) -> Result<(Vec<PartitionRecord>, Vec<String>)>
where
    R: Read + Seek,
{
    let mut records = Vec::new();
    let mut warnings = Vec::new();
    for entry in entries {
        if entry.is_extended() || entry.partition_type == 0 {
            continue;
        }
        let (record, warning) = probe_mbr_partition(reader, entry)?;
        if let Some(warning) = warning {
            warnings.push(warning);
        }
        records.push(record);
    }
    Ok((records, warnings))
}

fn probe_mbr_partition<R>(
    reader: &mut R,
    entry: &PartitionEntry,
) -> Result<(PartitionRecord, Option<String>)>
where
    R: Read + Seek,
{
    let offset = entry.lba_start as u64 * SECTOR_SIZE;
    let class = evidence_core::volume::mbr::classify_mbr_partition_type(entry.partition_type);
    let fs_kind = read_boot_filesystem(reader, offset)?;
    let kind_label = fs_kind
        .map(kind_label)
        .unwrap_or_else(|| class.name.to_string());
    let status = fs_kind.map_or_else(
        || partition_status_for_mbr_class(class.status),
        partition_status_for_filesystem,
    );
    let display_name =
        partition_display_name(entry.partition_number, &kind_label, None, Some(class.name));
    let warning = mbr_partition_warning(entry, &display_name, status);

    if status == PartitionStatus::Supported && matches!(fs_kind, Some(ImageFilesystemKind::LvmPool))
    {
        tracing::info!(
            "LVM2 physical volume detected at partition {} ({}), LV expansion deferred to import",
            entry.partition_number,
            display_name,
        );
    }

    Ok((
        PartitionRecord {
            index: entry.partition_number,
            name: display_name,
            kind_label,
            type_guid: None,
            offset,
            length: entry.sector_count as u64 * SECTOR_SIZE,
            status,
            filesystem: fs_kind,
            lvm_identity: None,
        },
        warning,
    ))
}

fn mbr_partition_warning(
    entry: &PartitionEntry,
    display_name: &str,
    status: PartitionStatus,
) -> Option<String> {
    match status {
        PartitionStatus::EncryptedBitLocker => Some(format!(
            "Partition {} '{}' is BitLocker-encrypted",
            entry.partition_number, display_name,
        )),
        PartitionStatus::Unsupported => Some(format!(
            "Partition {} '{}' is not yet supported (type 0x{:02X})",
            entry.partition_number, display_name, entry.partition_type,
        )),
        PartitionStatus::Supported | PartitionStatus::Expanded => None,
    }
}

fn partition_status_for_mbr_class(status: MbrPartitionStatus) -> PartitionStatus {
    match status {
        MbrPartitionStatus::Supported => PartitionStatus::Supported,
        MbrPartitionStatus::EncryptedBitLocker => PartitionStatus::EncryptedBitLocker,
        MbrPartitionStatus::Unsupported => PartitionStatus::Unsupported,
    }
}

pub(super) fn partition_status_for_filesystem(kind: ImageFilesystemKind) -> PartitionStatus {
    match kind {
        ImageFilesystemKind::BitLocker => PartitionStatus::EncryptedBitLocker,
        ImageFilesystemKind::Ntfs
        | ImageFilesystemKind::Fat
        | ImageFilesystemKind::Ext4
        | ImageFilesystemKind::Xfs
        | ImageFilesystemKind::Btrfs
        | ImageFilesystemKind::LvmPool => PartitionStatus::Supported,
    }
}

fn push_candidate(
    candidates: &mut Vec<ImageFilesystemCandidate>,
    partition_index: Option<usize>,
    partition_name: Option<String>,
    kind: ImageFilesystemKind,
    offset: u64,
    length: Option<u64>,
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
        length,
        source,
        lvm_identity: None,
    });
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
