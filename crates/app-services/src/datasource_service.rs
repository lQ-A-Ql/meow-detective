use domain::{
    CaseId, DataSource, DataSourceHashStatus, DataSourceId, DataSourceKind, DataSourceProvenance,
    DataSourceProvenanceStatus,
};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use thiserror::Error;

mod lvm;
pub(crate) use lvm::lvm_source_fingerprint;
pub use lvm::{expand_lvm_pool_candidates, expand_lvm_pool_candidates_with_sources};

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<DataSourceKind>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LvmDiscoverySource {
    pub source_path: PathBuf,
    pub source_kind: DataSourceKind,
}

impl LvmDiscoverySource {
    pub fn new(source_path: impl Into<PathBuf>, source_kind: DataSourceKind) -> Self {
        Self {
            source_path: source_path.into(),
            source_kind,
        }
    }
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

pub fn lvm_discovery_sources_for_case(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    current_data_source_id: Option<&DataSourceId>,
) -> Result<Vec<LvmDiscoverySource>> {
    let sources = DataSourceRepo::new(conn).find_by_case(case_id)?;
    Ok(sources
        .into_iter()
        .filter(|source| {
            current_data_source_id.is_none_or(|current_id| source.id != *current_id)
                && matches!(source.kind, DataSourceKind::E01 | DataSourceKind::Raw)
        })
        .map(|source| LvmDiscoverySource::new(source.source_path, source.kind))
        .collect())
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

pub(crate) fn normalize_lvm_uuid_for_match(uuid: &str) -> String {
    uuid.trim()
        .chars()
        .filter(|ch| *ch != '-')
        .collect::<String>()
        .to_ascii_lowercase()
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
    // LVM PV labels are authoritative when present. Probe them before
    // ordinary filesystem magics so stale signatures inside a PV do not bypass
    // LV expansion.
    match fs_lvm::probe_lvm(reader, offset) {
        Ok(true) => return Ok(Some(ImageFilesystemKind::LvmPool)),
        Ok(false) => {}
        Err(_e) => {
            tracing::debug!("LVM probe error at offset {}: {}", offset, _e);
        }
    }

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

    // Check for ext4 superblock magic at byte 0x38 within the superblock.
    reader.seek(SeekFrom::Start(offset + 1024 + 0x38))?;
    let mut sb = [0u8; 2];
    if reader.read_exact(&mut sb).is_ok() && u16::from_le_bytes(sb) == 0xEF53 {
        return Ok(Some(ImageFilesystemKind::Ext4));
    }

    // Check for Btrfs magic at byte 0x40 within the primary superblock.
    reader.seek(SeekFrom::Start(offset + 0x10000 + 0x40))?;
    let mut btrfs_magic = [0u8; 8];
    if reader.read_exact(&mut btrfs_magic).is_ok() && &btrfs_magic == b"_BHRfS_M" {
        return Ok(Some(ImageFilesystemKind::Btrfs));
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
    use super::lvm::lvm_pv_source_key;
    use super::*;
    use domain::CaseMeta;
    use persistence_sqlite::repositories::{case_repo::CaseRepo, datasource_repo::DataSourceRepo};
    use tempfile::TempDir;

    const SYNTHETIC_PV_SIZE: u64 = 2_097_152;
    const SYNTHETIC_PV_OFFSET: u64 = 1_048_576;
    const SYNTHETIC_DATA_AREA_START: u64 = 2560;
    const SYNTHETIC_PV0_UUID: &str = "00000000000000000000000000000000";
    const SYNTHETIC_PV1_UUID: &str = "11111111111111111111111111111111";

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
    fn read_boot_filesystem_detects_ext4_magic_inside_superblock() {
        let mut image = vec![0u8; 4096];
        image[1024 + 0x38..1024 + 0x3A].copy_from_slice(&0xEF53u16.to_le_bytes());

        let detected = read_boot_filesystem(&mut std::io::Cursor::new(image), 0).unwrap();

        assert_eq!(detected, Some(ImageFilesystemKind::Ext4));
    }

    #[test]
    fn read_boot_filesystem_detects_btrfs_magic_inside_superblock() {
        let mut image = vec![0u8; 0x11000];
        image[0x10000 + 0x40..0x10000 + 0x48].copy_from_slice(b"_BHRfS_M");

        let detected = read_boot_filesystem(&mut std::io::Cursor::new(image), 0).unwrap();

        assert_eq!(detected, Some(ImageFilesystemKind::Btrfs));
    }

    #[test]
    fn read_boot_filesystem_prefers_lvm_over_stale_xfs_magic() {
        let mut image = vec![0u8; 4096];
        let image_len = image.len() as u64;
        image[0..4].copy_from_slice(b"XFSB");
        let sector = &mut image[512..1024];
        sector[0..8].copy_from_slice(b"LABELONE");
        sector[8..16].copy_from_slice(&1u64.to_le_bytes());
        sector[20..24].copy_from_slice(&32u32.to_le_bytes());
        sector[24..32].copy_from_slice(b"LVM2 001");
        sector[32..64].copy_from_slice(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        sector[64..72].copy_from_slice(&image_len.to_le_bytes());
        sector[72..80].copy_from_slice(&2048u64.to_le_bytes());
        sector[80..88].copy_from_slice(&(image_len - 2048).to_le_bytes());
        let crc = fs_lvm::crc::lvm_crc32(&sector[20..512]);
        sector[16..20].copy_from_slice(&crc.to_le_bytes());

        let detected = read_boot_filesystem(&mut std::io::Cursor::new(image), 0).unwrap();

        assert_eq!(detected, Some(ImageFilesystemKind::LvmPool));
    }

    #[test]
    fn expand_lvm_pool_candidates_with_sources_groups_extra_pvs() {
        let tmp = TempDir::new().unwrap();
        let primary_path = tmp.path().join("pv0.raw");
        let extra_path = tmp.path().join("pv1.raw");
        let (mut pv0, pv1) = build_synthetic_multi_pv_lvm_disks();
        let data_area_start = SYNTHETIC_DATA_AREA_START as usize;
        pv0[data_area_start..data_area_start + 4].copy_from_slice(b"XFSB");
        std::fs::write(&primary_path, build_synthetic_lvm_mbr_image(&pv0)).unwrap();
        std::fs::write(&extra_path, build_synthetic_lvm_mbr_image(&pv1)).unwrap();

        let mut reader = evidence_core::RawImageReader::open(&primary_path).unwrap();
        let mut probe = detect_image_filesystem(&mut reader).unwrap();

        expand_lvm_pool_candidates_with_sources(
            &mut probe,
            &primary_path,
            &DataSourceKind::Raw,
            &[LvmDiscoverySource::new(&extra_path, DataSourceKind::Raw)],
        );

        let lvm_candidate = probe
            .candidates
            .iter()
            .find(|candidate| candidate.source == ImageFilesystemSource::LvmLogicalVolume)
            .unwrap_or_else(|| {
                panic!(
                    "expected expanded LVM logical volume candidate; candidates={:?}; warnings={:?}",
                    probe.candidates, probe.warnings
                )
            });
        assert_eq!(lvm_candidate.kind, ImageFilesystemKind::Xfs);
        let identity = lvm_candidate.lvm_identity.as_ref().unwrap();
        assert_eq!(identity.vg_name, "test_vg");
        assert_eq!(identity.lv_name, "root");
        assert_eq!(identity.pv_sources.len(), 2);
        assert_eq!(
            identity.pv_sources[0].source_path,
            primary_path.display().to_string()
        );
        assert_eq!(
            identity.pv_sources[0].source_kind,
            Some(DataSourceKind::Raw)
        );
        assert_eq!(identity.pv_sources[0].pv_uuid, SYNTHETIC_PV0_UUID);
        assert_eq!(identity.pv_sources[0].pv_name.as_deref(), Some("pv0"));
        assert_eq!(
            identity.pv_sources[1].source_path,
            extra_path.display().to_string()
        );
        assert_eq!(
            identity.pv_sources[1].source_kind,
            Some(DataSourceKind::Raw)
        );
        assert_eq!(identity.pv_sources[1].pv_uuid, SYNTHETIC_PV1_UUID);
        assert_eq!(identity.pv_sources[1].pv_name.as_deref(), Some("pv1"));
        assert!(probe
            .partitions
            .iter()
            .any(|partition| partition.status == PartitionStatus::Expanded));
        assert_eq!(
            probe
                .partitions
                .iter()
                .filter(|partition| partition.status == PartitionStatus::Expanded)
                .count(),
            1
        );
        assert_eq!(
            probe
                .partitions
                .iter()
                .find(|partition| partition.lvm_identity.is_some())
                .and_then(|partition| partition.lvm_identity.as_ref())
                .unwrap()
                .pv_sources,
            identity.pv_sources
        );
    }

    #[test]
    fn expand_lvm_pool_candidates_keeps_legacy_single_source_api() {
        let tmp = TempDir::new().unwrap();
        let primary_path = tmp.path().join("pv0.raw");
        let (mut pv0, _pv1) = build_synthetic_multi_pv_lvm_disks();
        let data_area_start = SYNTHETIC_DATA_AREA_START as usize;
        pv0[data_area_start..data_area_start + 4].copy_from_slice(b"XFSB");
        std::fs::write(&primary_path, build_synthetic_lvm_mbr_image(&pv0)).unwrap();

        let mut reader = evidence_core::RawImageReader::open(&primary_path).unwrap();
        let mut probe = detect_image_filesystem(&mut reader).unwrap();

        expand_lvm_pool_candidates(&mut probe, &primary_path, &DataSourceKind::Raw);

        assert!(probe
            .candidates
            .iter()
            .all(|candidate| candidate.source != ImageFilesystemSource::LvmLogicalVolume));
        assert!(
            probe
                .warnings
                .iter()
                .any(|warning| warning.contains("skipping incomplete")),
            "expected incomplete LVM warning, got {:?}",
            probe.warnings
        );
    }

    #[test]
    fn lvm_pv_source_key_includes_source_path_and_uuid() {
        let left = LvmPhysicalVolumeSource {
            source_path: "disk-a.E01".to_string(),
            source_kind: Some(DataSourceKind::E01),
            offset: 1_048_576,
            pv_uuid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            pv_name: Some("pv0".to_string()),
        };
        let same_offset_different_source = LvmPhysicalVolumeSource {
            source_path: "disk-b.E01".to_string(),
            source_kind: Some(DataSourceKind::E01),
            offset: 1_048_576,
            pv_uuid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            pv_name: Some("pv0".to_string()),
        };
        let same_offset_different_uuid = LvmPhysicalVolumeSource {
            source_path: "disk-a.E01".to_string(),
            source_kind: Some(DataSourceKind::E01),
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

    #[test]
    fn lvm_pv_source_key_includes_source_kind() {
        let e01 = LvmPhysicalVolumeSource {
            source_path: "disk.E01".to_string(),
            source_kind: Some(DataSourceKind::E01),
            offset: 0,
            pv_uuid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            pv_name: Some("pv0".to_string()),
        };
        let raw = LvmPhysicalVolumeSource {
            source_path: "disk.E01".to_string(),
            source_kind: Some(DataSourceKind::Raw),
            offset: 0,
            pv_uuid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            pv_name: Some("pv0".to_string()),
        };

        assert_ne!(lvm_pv_source_key(&e01), lvm_pv_source_key(&raw));
    }

    fn build_synthetic_multi_pv_lvm_disks() -> (Vec<u8>, Vec<u8>) {
        let metadata_text = format!(
            r#"test_vg {{
    id = "vg-multi-pv-1234"
    seqno = 2
    extent_size = 1

    physical_volumes {{
        pv0 {{
            id = "{}"
            device = "/dev/sda1"
            pe_start = 5
            pe_count = 16
        }}
        pv1 {{
            id = "{}"
            device = "/dev/sdb1"
            pe_start = 5
            pe_count = 16
        }}
    }}

    logical_volumes {{
        root {{
            id = "lv-root-uuid"
            status = ["READ","WRITE","VISIBLE"]
            segment_count = 1
            segment1 {{
                start_extent = 0
                extent_count = 1
                type = "linear"
                stripe_count = 1
                stripes = ["pv0", 0]
            }}
        }}
    }}
}}
"#,
            SYNTHETIC_PV0_UUID, SYNTHETIC_PV1_UUID
        );

        let mut pv0 = vec![0u8; SYNTHETIC_PV_SIZE as usize];
        let mut pv1 = vec![0u8; SYNTHETIC_PV_SIZE as usize];
        write_synthetic_lvm_pv_label(&mut pv0, SYNTHETIC_PV0_UUID);
        write_synthetic_lvm_pv_label(&mut pv1, SYNTHETIC_PV1_UUID);
        write_synthetic_lvm_metadata(&mut pv0, &metadata_text);
        (pv0, pv1)
    }

    fn build_synthetic_lvm_mbr_image(pv: &[u8]) -> Vec<u8> {
        let image_len = SYNTHETIC_PV_OFFSET as usize + pv.len();
        let mut image = vec![0u8; image_len];
        image[SYNTHETIC_PV_OFFSET as usize..].copy_from_slice(pv);
        write_synthetic_mbr_partition(&mut image, SYNTHETIC_PV_OFFSET / SECTOR_SIZE, pv.len());
        image
    }

    fn write_synthetic_mbr_partition(image: &mut [u8], lba_start: u64, byte_len: usize) {
        let sector_count = (byte_len as u64 / SECTOR_SIZE) as u32;
        let lba_start = lba_start as u32;
        let entry = &mut image[446..462];
        entry[4] = 0x8e;
        entry[8..12].copy_from_slice(&lba_start.to_le_bytes());
        entry[12..16].copy_from_slice(&sector_count.to_le_bytes());
        image[510] = 0x55;
        image[511] = 0xaa;
    }

    fn write_synthetic_lvm_pv_label(disk: &mut [u8], pv_uuid: &str) {
        let pv_size = disk.len() as u64;
        let sec = &mut disk[512..1024];
        sec[0..8].copy_from_slice(b"LABELONE");
        sec[8..16].copy_from_slice(&1u64.to_le_bytes());
        sec[20..24].copy_from_slice(&32u32.to_le_bytes());
        sec[24..32].copy_from_slice(b"LVM2 001");
        sec[32..64].copy_from_slice(format!("{pv_uuid:32}").as_bytes());
        sec[64..72].copy_from_slice(&pv_size.to_le_bytes());
        sec[72..80].copy_from_slice(&SYNTHETIC_DATA_AREA_START.to_le_bytes());
        sec[80..88].copy_from_slice(&(pv_size - SYNTHETIC_DATA_AREA_START).to_le_bytes());
        sec[104..112].copy_from_slice(&1024u64.to_le_bytes());
        sec[112..120].copy_from_slice(&(4 * 512u64).to_le_bytes());
        let crc = fs_lvm::crc::lvm_crc32(&sec[20..512]);
        sec[16..20].copy_from_slice(&crc.to_le_bytes());
    }

    fn write_synthetic_lvm_metadata(disk: &mut [u8], metadata_text: &str) {
        let text_bytes = metadata_text.as_bytes();
        let text_offset = 1536usize;
        let text_end = text_offset + text_bytes.len();
        assert!(text_end <= disk.len());

        {
            let mda = &mut disk[1024..1536];
            mda[4..20].copy_from_slice(b" LVM2 x[5A%r0N*>");
            mda[20..24].copy_from_slice(&1u32.to_le_bytes());
            mda[24..32].copy_from_slice(&1024u64.to_le_bytes());
            mda[32..40].copy_from_slice(&1536u64.to_le_bytes());
            mda[40..48].copy_from_slice(&512u64.to_le_bytes());
        }

        disk[text_offset..text_end].copy_from_slice(text_bytes);

        let text_size = text_bytes.len() as u64;
        let text_crc = fs_lvm::crc::lvm_crc32(text_bytes);
        {
            let mda = &mut disk[1024..1536];
            mda[48..56].copy_from_slice(&text_size.to_le_bytes());
            mda[56..60].copy_from_slice(&text_crc.to_le_bytes());
            let mda_crc = fs_lvm::crc::lvm_crc32(&mda[4..512]);
            mda[0..4].copy_from_slice(&mda_crc.to_le_bytes());
        }
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
