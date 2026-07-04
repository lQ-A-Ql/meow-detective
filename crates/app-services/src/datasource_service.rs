use domain::{
    CaseId, DataSource, DataSourceHashStatus, DataSourceId, DataSourceKind, DataSourceProvenance,
    DataSourceProvenanceStatus,
};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use thiserror::Error;

const SECTOR_SIZE: u64 = 512;

#[derive(Debug, Error)]
pub enum DataSourceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("Evidence error: {0}")]
    Evidence(String),
}

impl transport::ServiceErrorCategory for DataSourceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Io(_) | Self::Db(_) => transport::ErrorCategory::Io,
            Self::Evidence(_) => transport::ErrorCategory::Validation,
        }
    }
}

pub type Result<T> = std::result::Result<T, DataSourceError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFilesystemKind {
    Ntfs,
    Fat,
    BitLocker,
    Ext4,
    Xfs,
    Btrfs,
    LvmPool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFilesystemSource {
    DirectVolume,
    MbrPartition,
    GptPartition,
    LvmLogicalVolume,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LvmLogicalVolumeIdentity {
    pub vg_uuid: String,
    pub vg_name: String,
    pub lv_uuid: String,
    pub lv_name: String,
    pub pv_offsets: Vec<u64>,
    pub pv_sources: Vec<LvmPhysicalVolumeSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LvmPhysicalVolumeSource {
    pub source_path: String,
    pub offset: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pv_uuid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pv_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFilesystemCandidate {
    pub partition_index: Option<usize>,
    pub partition_name: Option<String>,
    pub kind: ImageFilesystemKind,
    pub offset: u64,
    pub source: ImageFilesystemSource,
    pub lvm_identity: Option<LvmLogicalVolumeIdentity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionStatus {
    Supported,
    Expanded,
    EncryptedBitLocker,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionRecord {
    pub index: usize,
    pub name: String,
    pub kind_label: String,
    pub type_guid: Option<String>,
    pub offset: u64,
    pub length: u64,
    pub status: PartitionStatus,
    pub filesystem: Option<ImageFilesystemKind>,
    pub lvm_identity: Option<LvmLogicalVolumeIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFilesystemProbe {
    pub candidates: Vec<ImageFilesystemCandidate>,
    pub partitions: Vec<PartitionRecord>,
    pub warnings: Vec<String>,
}

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

pub fn attach_data_source(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    name: &str,
    source_path: &Path,
    kind: DataSourceKind,
) -> Result<DataSource> {
    let id = DataSourceId(uuid::Uuid::new_v4().to_string());
    let provenance = build_attach_provenance(source_path, &kind);
    let ds = DataSource {
        id: id.clone(),
        name: name.to_string(),
        kind,
        source_path: source_path.to_path_buf(),
        imported_at: chrono::Utc::now(),
        provenance,
    };

    DataSourceRepo::new(conn).insert(case_id, &ds)?;
    Ok(ds)
}

fn build_attach_provenance(source_path: &Path, kind: &DataSourceKind) -> DataSourceProvenance {
    let mut warnings = Vec::new();
    let canonical_source_path = match std::fs::canonicalize(source_path) {
        Ok(path) => Some(path),
        Err(err) => {
            warnings.push(format!(
                "canonicalize failed for {}: {}",
                source_path.display(),
                err
            ));
            None
        }
    };
    let metadata = match std::fs::metadata(source_path) {
        Ok(metadata) => Some(metadata),
        Err(err) => {
            warnings.push(format!(
                "metadata unavailable for {}: {}",
                source_path.display(),
                err
            ));
            None
        }
    };
    let evidence_size = metadata
        .as_ref()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len());
    let hash_status = if metadata.as_ref().is_some_and(|metadata| metadata.is_file()) {
        DataSourceHashStatus::Pending
    } else if metadata.as_ref().is_some_and(|metadata| metadata.is_dir()) {
        DataSourceHashStatus::Unavailable
    } else {
        DataSourceHashStatus::Unknown
    };
    let provenance_status = if canonical_source_path.is_some() && metadata.is_some() {
        DataSourceProvenanceStatus::Recorded
    } else {
        DataSourceProvenanceStatus::Partial
    };

    DataSourceProvenance {
        source_hash_sha256: None,
        hash_status,
        canonical_source_path,
        evidence_size,
        reader_kind: Some(kind.to_string()),
        provenance_status,
        warnings,
    }
}

pub fn classify_data_source_path(source_path: &Path) -> Result<DataSourceKind> {
    let metadata = std::fs::metadata(source_path)?;
    if metadata.is_dir() {
        return Ok(DataSourceKind::LogicalDirectory);
    }

    if has_e01_magic(source_path)? || has_e01_name(source_path) {
        Ok(DataSourceKind::E01)
    } else {
        Ok(DataSourceKind::Raw)
    }
}

pub fn detect_image_filesystem<R>(reader: &mut R) -> Result<ImageFilesystemProbe>
where
    R: Read + Seek,
{
    let mut warnings = Vec::new();
    let sector0 = read_sector(reader, 0)?;
    let mut candidates = Vec::new();
    let mut partitions = Vec::new();

    if let Some(kind) = detect_boot_filesystem(&sector0) {
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

    // Push filesystem candidates from primary + logical partitions
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
    // (only when not falling through to GPT — GPT produces its own records)
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
                // LVM pool detected — log discovery, expansion happens in import pipeline
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

fn has_e01_magic(source_path: &Path) -> Result<bool> {
    let mut file = std::fs::File::open(source_path)?;
    let mut magic = [0u8; 8];
    match file.read_exact(&mut magic) {
        Ok(()) => Ok(&magic == b"EVF\x09\x0d\x0a\xff\x00" || &magic[0..3] == b"EVF"),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn has_e01_name(source_path: &Path) -> bool {
    let extension = source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "e01" | "ewf") {
        return true;
    }

    source_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase().contains(".e01."))
        .unwrap_or(false)
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

/// Expand LVM pool candidates into individual logical volume candidates.
///
/// Groups `LvmPool` candidates by VG metadata before discovery so an
/// incomplete/high-seqno VG cannot prevent a separate complete VG from
/// expanding.
///
/// Call after `detect_image_filesystem` and before storing partition records.
pub fn expand_lvm_pool_candidates(
    probe: &mut ImageFilesystemProbe,
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
) {
    let lvm_indices: Vec<(usize, ImageFilesystemCandidate)> = probe
        .candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c.kind, ImageFilesystemKind::LvmPool))
        .map(|(i, c)| (i, c.clone()))
        .collect();

    if lvm_indices.is_empty() {
        return;
    }

    let mut new_candidates: Vec<(ImageFilesystemCandidate, u64)> = Vec::new();
    let mut remove_indices: Vec<usize> = Vec::new();
    let mut expanded_vgs = std::collections::HashSet::new();
    let discovery_groups =
        lvm_discovery_pv_groups(&lvm_indices, source_path, source_kind, &mut probe.warnings);

    for pv_sources in discovery_groups {
        let pv_offsets = pv_sources
            .iter()
            .map(|source| source.offset)
            .collect::<Vec<_>>();
        let seed_offset = pv_offsets.first().copied().unwrap_or_default();
        let mut readers = Vec::with_capacity(pv_sources.len());
        for pv_source in &pv_sources {
            let reader_path = std::path::Path::new(&pv_source.source_path);
            match open_evidence_reader(reader_path, source_kind) {
                Ok(reader) => readers.push(reader),
                Err(e) => {
                    probe.warnings.push(format!(
                        "LVM expand: cannot open reader for PV '{}' offset {}: {}",
                        reader_path.display(),
                        pv_source.offset,
                        e
                    ));
                    tracing::warn!(
                        "LVM expand: cannot open reader for PV '{}' at offset {}: {}",
                        reader_path.display(),
                        pv_source.offset,
                        e
                    );
                    readers.clear();
                    break;
                }
            }
        }
        if readers.is_empty() {
            continue;
        }

        // Discover the volume group
        let pool = match fs_lvm::LvmPool::discover(readers, pv_offsets.clone()) {
            Ok(p) => p,
            Err(e) => {
                probe.warnings.push(format!(
                    "LVM expand: discovery failed for PV offsets {:?}: {}",
                    pv_offsets, e
                ));
                tracing::warn!(
                    "LVM expand: discovery failed at offsets {:?}: {}",
                    pv_offsets,
                    e
                );
                continue;
            }
        };

        let vg_pv_mappings = pool
            .physical_volume_offsets()
            .iter()
            .map(|(pv_name, offset)| (pv_name.clone(), *offset))
            .collect::<Vec<_>>();
        let expanded_offsets = if vg_pv_mappings.is_empty() {
            vec![seed_offset]
        } else {
            vg_pv_mappings
                .iter()
                .map(|(_, offset)| *offset)
                .collect::<Vec<_>>()
        };
        let expanded_sources =
            lvm_sources_for_pv_mappings(&pv_sources, &vg_pv_mappings, &expanded_offsets);
        let representative =
            representative_lvm_candidate(&lvm_indices, &expanded_offsets).or_else(|| {
                lvm_indices
                    .iter()
                    .find(|(_, candidate)| candidate.offset == seed_offset)
                    .map(|(_, candidate)| candidate)
            });

        let vg = pool.volume_group();
        let vg_key = if vg.id.is_empty() {
            vg.name.clone()
        } else {
            vg.id.clone()
        };
        if !expanded_vgs.insert(vg_key) {
            continue;
        }

        let lv_list = pool.list_volumes();
        tracing::info!(
            "LVM: {} logical volume(s) discovered at offset {}",
            lv_list.len(),
            expanded_offsets.first().copied().unwrap_or(seed_offset),
        );
        for lv_info in &lv_list {
            if !lv_info.directly_mappable {
                let reason = lv_info
                    .unsupported_reason
                    .as_deref()
                    .unwrap_or("unsupported logical volume mapping");
                probe.warnings.push(format!(
                    "LVM expand: skipping logical volume '{}/{}' role='{}': {}",
                    vg.name, lv_info.name, lv_info.role, reason
                ));
                tracing::debug!(
                    "LVM: skipping logical volume '{}' role='{}': {}",
                    lv_info.name,
                    lv_info.role,
                    reason
                );
            }
        }

        // Open each LV from the shared pool (no re-open needed)
        let candidates_before = new_candidates.len();
        for (lv_idx, lv_info) in pool.list_direct_volumes() {
            let mut lv_reader = match pool.open_volume(lv_idx) {
                Ok(r) => r,
                Err(e) => {
                    probe.warnings.push(format!(
                        "LVM expand: open logical volume '{}/{}' failed: {}",
                        vg.name, lv_info.name, e
                    ));
                    tracing::warn!("LVM: open_volume '{}' failed: {}", lv_info.name, e);
                    continue;
                }
            };

            // Detect filesystem on the LV
            match read_boot_filesystem(&mut lv_reader, 0) {
                Ok(Some(fs_kind)) if !matches!(fs_kind, ImageFilesystemKind::LvmPool) => {
                    let lv_name = format!(
                        "{}/{}",
                        if vg.name.is_empty() {
                            representative
                                .and_then(|candidate| candidate.partition_name.as_deref())
                                .unwrap_or("LVM")
                        } else {
                            vg.name.as_str()
                        },
                        lv_info.name
                    );
                    let lvm_identity = LvmLogicalVolumeIdentity {
                        vg_uuid: vg.id.clone(),
                        vg_name: vg.name.clone(),
                        lv_uuid: lv_info.uuid.clone(),
                        lv_name: lv_info.name.clone(),
                        pv_offsets: expanded_offsets.clone(),
                        pv_sources: expanded_sources.clone(),
                    };
                    new_candidates.push((
                        ImageFilesystemCandidate {
                            partition_index: representative
                                .and_then(|candidate| candidate.partition_index),
                            partition_name: Some(lv_name),
                            kind: fs_kind,
                            offset: expanded_offsets.first().copied().unwrap_or(seed_offset),
                            source: ImageFilesystemSource::LvmLogicalVolume,
                            lvm_identity: Some(lvm_identity),
                        },
                        lv_info.size_bytes,
                    ));
                }
                Ok(_) => {
                    probe.warnings.push(format!(
                        "LVM expand: logical volume '{}/{}' has no supported filesystem",
                        vg.name, lv_info.name
                    ));
                    tracing::debug!(
                        "LVM LV '{}': no supported filesystem detected, skipping",
                        lv_info.name
                    );
                }
                Err(e) => {
                    probe.warnings.push(format!(
                        "LVM expand: filesystem detection failed for logical volume '{}/{}': {}",
                        vg.name, lv_info.name, e
                    ));
                    tracing::debug!("LVM LV '{}': FS detection error: {}", lv_info.name, e);
                }
            }
        }

        mark_lvm_partitions_expanded(probe, &expanded_offsets);
        remove_lvm_candidates_for_offsets(&mut remove_indices, &lvm_indices, &expanded_offsets);
        if new_candidates.len() == candidates_before {
            probe.warnings.push(format!(
                "LVM expand: volume group '{}' produced no supported logical volume candidates",
                vg.name
            ));
        }
    }

    // Remove original LvmPool candidates (descending index order)
    remove_indices.sort_unstable_by(|a, b| b.cmp(a));
    for idx in &remove_indices {
        probe.candidates.remove(*idx);
    }

    // Add partition records for each new LV candidate; fix candidate indices
    // to match their PartitionRecord so build_partition_work can find them.
    let next_index = probe.partitions.iter().map(|p| p.index).max().unwrap_or(0) + 1;
    for (i, (lv_candidate, lv_size_bytes)) in new_candidates.iter_mut().enumerate() {
        let lv_index = next_index + i;
        lv_candidate.partition_index = Some(lv_index);
        probe.partitions.push(PartitionRecord {
            index: lv_index,
            name: lv_candidate
                .partition_name
                .clone()
                .unwrap_or_else(|| format!("LV_{}", lv_index)),
            kind_label: kind_label(lv_candidate.kind),
            type_guid: None,
            offset: lv_candidate.offset,
            length: *lv_size_bytes,
            status: PartitionStatus::Supported,
            filesystem: Some(lv_candidate.kind),
            lvm_identity: lv_candidate.lvm_identity.clone(),
        });
    }

    probe
        .candidates
        .extend(new_candidates.into_iter().map(|(candidate, _)| candidate));
}

#[derive(Clone)]
struct LvmPvDiscoveryInfo {
    source: LvmPhysicalVolumeSource,
    label: fs_lvm::LvmLabel,
    volume_group: Option<fs_lvm::VolumeGroup>,
    metadata_warnings: Vec<String>,
}

struct LvmMetadataGroup {
    volume_group: fs_lvm::VolumeGroup,
}

fn lvm_discovery_pv_groups(
    lvm_indices: &[(usize, ImageFilesystemCandidate)],
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
    warnings: &mut Vec<String>,
) -> Vec<Vec<LvmPhysicalVolumeSource>> {
    let mut pv_infos = Vec::new();
    let mut fallback_offsets = Vec::new();
    let default_source_path = source_path.to_string_lossy().into_owned();
    for (_, candidate) in lvm_indices {
        match inspect_lvm_pv_candidate(candidate, source_path, source_kind) {
            Ok(info) => pv_infos.push(info),
            Err(warning) => {
                warnings.push(warning);
                fallback_offsets.push(vec![LvmPhysicalVolumeSource {
                    source_path: default_source_path.clone(),
                    offset: candidate.offset,
                    pv_uuid: String::new(),
                    pv_name: None,
                }]);
            }
        }
    }

    for info in &pv_infos {
        warnings.extend(info.metadata_warnings.iter().cloned());
    }

    let mut metadata_groups = std::collections::BTreeMap::<String, LvmMetadataGroup>::new();
    for info in &pv_infos {
        let Some(volume_group) = info.volume_group.as_ref() else {
            continue;
        };
        let key = lvm_volume_group_key(volume_group);
        metadata_groups
            .entry(key)
            .and_modify(|group| {
                if volume_group.seqno > group.volume_group.seqno {
                    group.volume_group = volume_group.clone();
                }
            })
            .or_insert_with(|| LvmMetadataGroup {
                volume_group: volume_group.clone(),
            });
    }

    let mut grouped_offsets = std::collections::HashSet::new();
    let mut groups = Vec::new();
    for group in metadata_groups.values() {
        let mut sources = Vec::new();
        let mut missing_pv_uuids = Vec::new();
        for pv_meta in &group.volume_group.physical_volumes {
            let required_uuid = normalize_lvm_uuid_for_match(&pv_meta.uuid);
            let matched = pv_infos
                .iter()
                .find(|info| normalize_lvm_uuid_for_match(&info.label.pv_uuid) == required_uuid);
            match matched {
                Some(info) => {
                    if !sources.iter().any(|source: &LvmPhysicalVolumeSource| {
                        lvm_pv_source_key(source) == lvm_pv_source_key(&info.source)
                    }) {
                        let mut source = info.source.clone();
                        source.pv_name = Some(pv_meta.name.clone());
                        sources.push(source);
                    }
                }
                None => missing_pv_uuids.push(pv_meta.uuid.clone()),
            }
        }

        if missing_pv_uuids.is_empty() && !sources.is_empty() {
            for source in &sources {
                grouped_offsets.insert(lvm_pv_source_key(source));
            }
            groups.push(sources);
        } else if !missing_pv_uuids.is_empty() {
            warnings.push(format!(
                "LVM expand: skipping incomplete VG '{}' missing PV UUID(s): {}",
                lvm_volume_group_display_name(&group.volume_group),
                missing_pv_uuids.join(", ")
            ));
        }
    }

    for info in pv_infos {
        if info.volume_group.is_none() && grouped_offsets.insert(lvm_pv_source_key(&info.source)) {
            groups.push(vec![info.source]);
        }
    }
    groups.extend(fallback_offsets);
    groups
}

fn inspect_lvm_pv_candidate(
    candidate: &ImageFilesystemCandidate,
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
) -> std::result::Result<LvmPvDiscoveryInfo, String> {
    let mut reader = open_evidence_reader(source_path, source_kind).map_err(|e| {
        format!(
            "LVM expand: cannot open reader for PV offset {}: {}",
            candidate.offset, e
        )
    })?;
    let label = fs_lvm::label::parse_pv_label(&mut reader, candidate.offset).map_err(|e| {
        format!(
            "LVM expand: cannot parse PV label at offset {}: {}",
            candidate.offset, e
        )
    })?;
    let (volume_group, metadata_warnings) =
        best_lvm_volume_group_from_label(&mut reader, candidate.offset, &label);
    let source = LvmPhysicalVolumeSource {
        source_path: source_path.to_string_lossy().into_owned(),
        offset: candidate.offset,
        pv_uuid: label.pv_uuid.clone(),
        pv_name: None,
    };
    Ok(LvmPvDiscoveryInfo {
        source,
        label,
        volume_group,
        metadata_warnings,
    })
}

fn best_lvm_volume_group_from_label<R>(
    reader: &mut R,
    pv_offset: u64,
    label: &fs_lvm::LvmLabel,
) -> (Option<fs_lvm::VolumeGroup>, Vec<String>)
where
    R: Read + Seek,
{
    let mut best = None;
    let mut warnings = Vec::new();
    for (index, metadata_area) in label.metadata_areas.iter().enumerate() {
        match fs_lvm::metadata::parse_metadata(reader, metadata_area, pv_offset) {
            Ok(volume_group) => {
                if best
                    .as_ref()
                    .is_none_or(|current: &fs_lvm::VolumeGroup| volume_group.seqno > current.seqno)
                {
                    best = Some(volume_group);
                }
            }
            Err(error) => {
                warnings.push(format!(
                    "LVM expand: metadata area {} at PV offset {} (mda offset {}, size {}) did not produce a usable VG: {}",
                    index,
                    pv_offset,
                    metadata_area.offset,
                    metadata_area.size,
                    error
                ));
            }
        }
    }
    (best, warnings)
}

fn lvm_volume_group_key(volume_group: &fs_lvm::VolumeGroup) -> String {
    let normalized_id = normalize_lvm_uuid_for_match(&volume_group.id);
    if normalized_id.is_empty() {
        format!("name:{}", volume_group.name)
    } else {
        format!("id:{normalized_id}")
    }
}

fn lvm_volume_group_display_name(volume_group: &fs_lvm::VolumeGroup) -> String {
    if volume_group.name.is_empty() {
        volume_group.id.clone()
    } else {
        volume_group.name.clone()
    }
}

pub(crate) fn normalize_lvm_uuid_for_match(uuid: &str) -> String {
    uuid.trim()
        .chars()
        .filter(|ch| *ch != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

fn lvm_pv_source_key(source: &LvmPhysicalVolumeSource) -> (String, u64, String) {
    (
        source.source_path.clone(),
        source.offset,
        normalize_lvm_uuid_for_match(&source.pv_uuid),
    )
}

fn representative_lvm_candidate<'a>(
    lvm_indices: &'a [(usize, ImageFilesystemCandidate)],
    pv_offsets: &[u64],
) -> Option<&'a ImageFilesystemCandidate> {
    for offset in pv_offsets {
        if let Some((_, candidate)) = lvm_indices
            .iter()
            .find(|(_, candidate)| candidate.offset == *offset)
        {
            return Some(candidate);
        }
    }
    None
}

fn lvm_sources_for_pv_mappings(
    sources: &[LvmPhysicalVolumeSource],
    pv_mappings: &[(String, u64)],
    pv_offsets: &[u64],
) -> Vec<LvmPhysicalVolumeSource> {
    if !pv_mappings.is_empty() {
        return pv_mappings
            .iter()
            .filter_map(|(pv_name, offset)| {
                sources
                    .iter()
                    .find(|source| {
                        source.offset == *offset
                            && source.pv_name.as_deref().is_none_or(|name| name == pv_name)
                    })
                    .cloned()
                    .map(|mut source| {
                        source.pv_name = Some(pv_name.clone());
                        source
                    })
            })
            .collect();
    }

    pv_offsets
        .iter()
        .filter_map(|offset| sources.iter().find(|source| source.offset == *offset))
        .cloned()
        .collect()
}

fn open_evidence_reader(
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
) -> std::io::Result<Box<dyn evidence_core::EvidenceReader>> {
    match source_kind {
        domain::DataSourceKind::E01 => image_e01::E01Reader::open(source_path)
            .map(|r| Box::new(r) as Box<dyn evidence_core::EvidenceReader>),
        _ => evidence_core::RawImageReader::open(source_path)
            .map(|r| Box::new(r) as Box<dyn evidence_core::EvidenceReader>),
    }
}

fn remove_lvm_candidates_for_offsets(
    remove_indices: &mut Vec<usize>,
    lvm_indices: &[(usize, ImageFilesystemCandidate)],
    pv_offsets: &[u64],
) {
    for (idx, candidate) in lvm_indices {
        if pv_offsets.contains(&candidate.offset) && !remove_indices.contains(idx) {
            remove_indices.push(*idx);
        }
    }
}

fn mark_lvm_partitions_expanded(probe: &mut ImageFilesystemProbe, pv_offsets: &[u64]) {
    for partition in &mut probe.partitions {
        if pv_offsets.contains(&partition.offset)
            && matches!(partition.filesystem, Some(ImageFilesystemKind::LvmPool))
        {
            partition.status = PartitionStatus::Expanded;
        }
    }
}

/// Assign effective partition indices for candidates where `partition_index` is `None`
/// (typical for MBR disks). Candidates are sorted by offset so that indices are
/// deterministic and consistent across probe, import, and viewer paths.
///
/// Returns a map from candidate position → effective index. Candidates that already
/// have a `partition_index` are left unchanged (not included in the map).
pub fn assign_effective_partition_indices(
    candidates: &[ImageFilesystemCandidate],
) -> std::collections::HashMap<usize, usize> {
    let mut offsets: Vec<(usize, u64)> = candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| c.partition_index.is_none())
        .map(|(i, c)| (i, c.offset))
        .collect();
    offsets.sort_by_key(|(_, o)| *o);
    let mut map = std::collections::HashMap::new();
    for (unique_idx, (orig_pos, _)) in offsets.iter().enumerate() {
        map.insert(*orig_pos, unique_idx);
    }
    map
}

/// Resolve the effective partition index for a candidate, using the precomputed
/// map from `assign_effective_partition_indices`.
pub fn effective_partition_index(
    candidate: &ImageFilesystemCandidate,
    candidate_pos: usize,
    index_map: &std::collections::HashMap<usize, usize>,
) -> usize {
    candidate
        .partition_index
        .unwrap_or_else(|| *index_map.get(&candidate_pos).unwrap_or(&0))
}

fn kind_label(kind: ImageFilesystemKind) -> String {
    match kind {
        ImageFilesystemKind::Ntfs => "NTFS".to_string(),
        ImageFilesystemKind::Fat => "FAT".to_string(),
        ImageFilesystemKind::BitLocker => "BitLocker".to_string(),
        ImageFilesystemKind::Ext4 => "Ext4".to_string(),
        ImageFilesystemKind::Xfs => "XFS".to_string(),
        ImageFilesystemKind::Btrfs => "Btrfs".to_string(),
        ImageFilesystemKind::LvmPool => "LVM".to_string(),
    }
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

fn looks_like_bitlocker_boot_sector(sector: &[u8; 512]) -> bool {
    &sector[3..11] == b"-FVE-FS-"
}

fn detect_boot_filesystem(sector: &[u8; 512]) -> Option<ImageFilesystemKind> {
    if &sector[3..11] == b"NTFS    " {
        return Some(ImageFilesystemKind::Ntfs);
    }

    if looks_like_bitlocker_boot_sector(sector) {
        return Some(ImageFilesystemKind::BitLocker);
    }

    if looks_like_fat_boot_sector(sector) {
        return Some(ImageFilesystemKind::Fat);
    }

    None
}

fn read_boot_filesystem<R>(reader: &mut R, offset: u64) -> Result<Option<ImageFilesystemKind>>
where
    R: Read + Seek,
{
    let sector = read_sector(reader, offset)?;
    if let Some(kind) = detect_boot_filesystem(&sector) {
        return Ok(Some(kind));
    }

    // Check for XFS at sector 0 (big-endian magic "XFSB")
    if offset.is_multiple_of(512) {
        let magic = u32::from_be_bytes([sector[0], sector[1], sector[2], sector[3]]);
        if magic == 0x5846_5342 {
            return Ok(Some(ImageFilesystemKind::Xfs));
        }
    }

    // Check for ext4 superblock at offset 1024 within the partition
    reader.seek(SeekFrom::Start(offset + 1024))?;
    let mut sb = [0u8; 2];
    if reader.read_exact(&mut sb).is_ok() && u16::from_le_bytes(sb) == 0xEF53 {
        return Ok(Some(ImageFilesystemKind::Ext4));
    }

    // Check for Btrfs superblock at offset 0x10000 within the partition
    reader.seek(SeekFrom::Start(offset + 0x10000))?;
    let mut btrfs_magic = [0u8; 8];
    if reader.read_exact(&mut btrfs_magic).is_ok() && &btrfs_magic == b"_BHRfS_M" {
        return Ok(Some(ImageFilesystemKind::Btrfs));
    }

    // Check for LVM2 PV label at sector 1 of the partition
    match fs_lvm::probe_lvm(reader, offset) {
        Ok(true) => return Ok(Some(ImageFilesystemKind::LvmPool)),
        Ok(false) => {}
        Err(_e) => {
            tracing::debug!("LVM probe error at offset {}: {}", offset, _e);
        }
    }

    Ok(None)
}

fn read_sector<R>(reader: &mut R, offset: u64) -> Result<[u8; 512]>
where
    R: Read + Seek,
{
    let mut sector = [0u8; 512];
    reader.seek(SeekFrom::Start(offset))?;
    reader.read_exact(&mut sector)?;
    Ok(sector)
}

fn looks_like_fat_boot_sector(sector: &[u8; 512]) -> bool {
    if sector[510] != 0x55 || sector[511] != 0xAA {
        return false;
    }

    let bytes_per_sector = u16::from_le_bytes([sector[11], sector[12]]);
    if !matches!(bytes_per_sector, 512 | 1024 | 2048 | 4096) {
        return false;
    }

    let sectors_per_cluster = sector[13];
    if sectors_per_cluster == 0 || !sectors_per_cluster.is_power_of_two() {
        return false;
    }

    let reserved_sectors = u16::from_le_bytes([sector[14], sector[15]]);
    let fat_count = sector[16];
    if reserved_sectors == 0 || fat_count == 0 || fat_count > 2 {
        return false;
    }

    let fat16_label = &sector[54..62];
    let fat32_label = &sector[82..90];
    if matches!(fat16_label, b"FAT12   " | b"FAT16   ") || fat32_label == b"FAT32   " {
        return true;
    }

    let total16 = u16::from_le_bytes([sector[19], sector[20]]);
    let total32 = u32::from_le_bytes(sector[32..36].try_into().unwrap_or([0; 4]));
    let fat16_sectors = u16::from_le_bytes([sector[22], sector[23]]);
    let fat32_sectors = u32::from_le_bytes(sector[36..40].try_into().unwrap_or([0; 4]));

    (total16 != 0 || total32 != 0) && (fat16_sectors != 0 || fat32_sectors != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::CaseMeta;
    use persistence_sqlite::repositories::{case_repo::CaseRepo, datasource_repo::DataSourceRepo};
    use tempfile::TempDir;

    fn setup_case() -> (rusqlite::Connection, CaseId) {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        persistence_sqlite::runner::run_all(&conn).unwrap();
        let case = CaseMeta {
            id: CaseId("case-datasource".to_string()),
            name: "DataSource Test".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        CaseRepo::new(&conn).create(&case).unwrap();
        (conn, case.id)
    }

    #[test]
    fn attach_data_source_records_file_provenance() {
        let tmp = TempDir::new().unwrap();
        let source_path = tmp.path().join("sample.raw");
        std::fs::write(&source_path, b"sample evidence").unwrap();
        let (conn, case_id) = setup_case();

        let attached =
            attach_data_source(&conn, &case_id, "sample", &source_path, DataSourceKind::Raw)
                .unwrap();
        let stored = DataSourceRepo::new(&conn)
            .find_by_case(&case_id)
            .unwrap()
            .into_iter()
            .find(|source| source.id == attached.id)
            .unwrap();

        assert_eq!(stored.provenance.source_hash_sha256, None);
        assert_eq!(stored.provenance.hash_status, DataSourceHashStatus::Pending);
        assert_eq!(stored.provenance.evidence_size, Some(15));
        assert_eq!(stored.provenance.reader_kind.as_deref(), Some("raw"));
        assert_eq!(
            stored.provenance.provenance_status,
            DataSourceProvenanceStatus::Recorded
        );
        assert_eq!(
            stored.provenance.canonical_source_path,
            Some(std::fs::canonicalize(&source_path).unwrap())
        );
        assert!(stored.provenance.warnings.is_empty());
    }

    #[test]
    fn attach_data_source_records_directory_provenance_without_size() {
        let tmp = TempDir::new().unwrap();
        let source_path = tmp.path().join("logical-evidence");
        std::fs::create_dir(&source_path).unwrap();
        let (conn, case_id) = setup_case();

        let attached = attach_data_source(
            &conn,
            &case_id,
            "logical",
            &source_path,
            DataSourceKind::LogicalDirectory,
        )
        .unwrap();
        let stored = DataSourceRepo::new(&conn)
            .find_by_case(&case_id)
            .unwrap()
            .into_iter()
            .find(|source| source.id == attached.id)
            .unwrap();

        assert_eq!(
            stored.provenance.hash_status,
            DataSourceHashStatus::Unavailable
        );
        assert_eq!(stored.provenance.evidence_size, None);
        assert_eq!(
            stored.provenance.reader_kind.as_deref(),
            Some("logical_directory")
        );
        assert_eq!(
            stored.provenance.provenance_status,
            DataSourceProvenanceStatus::Recorded
        );
        assert_eq!(
            stored.provenance.canonical_source_path,
            Some(std::fs::canonicalize(&source_path).unwrap())
        );
        assert!(stored.provenance.warnings.is_empty());
    }

    #[test]
    fn lvm_pv_source_key_includes_source_path_and_uuid() {
        let left = LvmPhysicalVolumeSource {
            source_path: "disk-a.E01".to_string(),
            offset: 1_048_576,
            pv_uuid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            pv_name: Some("pv0".to_string()),
        };
        let same_offset_different_source = LvmPhysicalVolumeSource {
            source_path: "disk-b.E01".to_string(),
            offset: 1_048_576,
            pv_uuid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            pv_name: Some("pv0".to_string()),
        };
        let same_offset_different_uuid = LvmPhysicalVolumeSource {
            source_path: "disk-a.E01".to_string(),
            offset: 1_048_576,
            pv_uuid: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            pv_name: Some("pv1".to_string()),
        };

        assert_ne!(
            lvm_pv_source_key(&left),
            lvm_pv_source_key(&same_offset_different_source)
        );
        assert_ne!(
            lvm_pv_source_key(&left),
            lvm_pv_source_key(&same_offset_different_uuid)
        );
    }

    fn candidate(offset: u64, partition_index: Option<usize>) -> ImageFilesystemCandidate {
        ImageFilesystemCandidate {
            partition_index,
            partition_name: None,
            kind: ImageFilesystemKind::Ntfs,
            offset,
            source: ImageFilesystemSource::MbrPartition,
            lvm_identity: None,
        }
    }

    #[test]
    fn assign_indices_sorts_by_offset() {
        let candidates = vec![
            candidate(3000, None),
            candidate(1000, None),
            candidate(2000, None),
        ];
        let map = assign_effective_partition_indices(&candidates);
        // sorted by offset: 1000→idx0, 2000→idx1, 3000→idx2
        assert_eq!(effective_partition_index(&candidates[0], 0, &map), 2);
        assert_eq!(effective_partition_index(&candidates[1], 1, &map), 0);
        assert_eq!(effective_partition_index(&candidates[2], 2, &map), 1);
    }

    #[test]
    fn assign_indices_preserves_existing() {
        let candidates = vec![
            candidate(2000, Some(5)),
            candidate(1000, None),
            candidate(3000, None),
        ];
        let map = assign_effective_partition_indices(&candidates);
        // existing index preserved
        assert_eq!(effective_partition_index(&candidates[0], 0, &map), 5);
        // sorted by offset: 1000→idx0, 3000→idx1
        assert_eq!(effective_partition_index(&candidates[1], 1, &map), 0);
        assert_eq!(effective_partition_index(&candidates[2], 2, &map), 1);
    }

    #[test]
    fn assign_indices_single_candidate() {
        let candidates = vec![candidate(500, None)];
        let map = assign_effective_partition_indices(&candidates);
        assert_eq!(effective_partition_index(&candidates[0], 0, &map), 0);
    }

    #[test]
    fn assign_indices_empty() {
        let candidates: Vec<ImageFilesystemCandidate> = vec![];
        let map = assign_effective_partition_indices(&candidates);
        assert!(map.is_empty());
    }
}
