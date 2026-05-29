use domain::{CaseId, DataSource, DataSourceId, DataSourceKind};
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

pub type Result<T> = std::result::Result<T, DataSourceError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFilesystemKind {
    Ntfs,
    Fat,
    BitLocker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFilesystemSource {
    DirectVolume,
    MbrPartition,
    GptPartition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFilesystemCandidate {
    pub partition_index: Option<usize>,
    pub partition_name: Option<String>,
    pub kind: ImageFilesystemKind,
    pub offset: u64,
    pub source: ImageFilesystemSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionStatus {
    Supported,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFilesystemProbe {
    pub candidates: Vec<ImageFilesystemCandidate>,
    pub partitions: Vec<PartitionRecord>,
    pub warnings: Vec<String>,
}

pub fn attach_data_source(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    name: &str,
    source_path: &Path,
    kind: DataSourceKind,
) -> Result<DataSource> {
    let id = DataSourceId(uuid::Uuid::new_v4().to_string());
    let ds = DataSource {
        id: id.clone(),
        name: name.to_string(),
        kind,
        source_path: source_path.to_path_buf(),
        imported_at: chrono::Utc::now(),
    };

    DataSourceRepo::new(conn).insert(case_id, &ds)?;
    Ok(ds)
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
                },
                filesystem: Some(kind),
            }],
            warnings,
        });
    }

    let mbr_entries = evidence_core::volume::mbr::parse_partition_table(&sector0);
    let mbr_types: Vec<String> = mbr_entries
        .iter()
        .filter(|entry| entry.partition_type != 0)
        .map(|entry| format!("{:02X}", entry.partition_type))
        .collect();

    for entry in mbr_entries
        .iter()
        .filter(|entry| entry.partition_type != 0 && entry.lba_start > 0)
    {
        let offset = entry.lba_start as u64 * SECTOR_SIZE;
        if let Some(kind) = read_boot_filesystem(reader, offset)? {
            push_candidate(
                &mut candidates,
                None,
                None,
                kind,
                offset,
                ImageFilesystemSource::MbrPartition,
            );
        }
    }

    if mbr_entries.iter().any(|entry| entry.partition_type == 0xEE) {
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
        let trimmed_name = partition.name.trim();
        let display_name = if trimmed_name.is_empty() {
            format!("Partition {}", partition.index)
        } else {
            trimmed_name.to_string()
        };
        let mut status = PartitionStatus::Unsupported;

        if let Some(kind) = fs_kind {
            status = match kind {
                ImageFilesystemKind::Ntfs | ImageFilesystemKind::Fat => PartitionStatus::Supported,
                ImageFilesystemKind::BitLocker => PartitionStatus::EncryptedBitLocker,
            };
        }

        if let Some(kind) = fs_kind {
            if matches!(kind, ImageFilesystemKind::Ntfs | ImageFilesystemKind::Fat) {
                candidates.push(ImageFilesystemCandidate {
                    partition_index: Some(partition.index),
                    partition_name: Some(display_name.clone()),
                    kind,
                    offset,
                    source: ImageFilesystemSource::GptPartition,
                });
            }
        }

        if status == PartitionStatus::EncryptedBitLocker {
            warnings.push(format!(
                "Partition {} '{}' is BitLocker-encrypted and currently locked",
                partition.index, display_name
            ));
        } else if status == PartitionStatus::Unsupported {
            warnings.push(format!(
                "Partition {} '{}' is not yet supported ({}, GUID {})",
                partition.index,
                display_name,
                type_name,
                evidence_core::volume::gpt::format_guid(&partition.type_guid)
            ));
        }

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
        });
    }

    Ok(ImageFilesystemProbe {
        candidates,
        partitions: records,
        warnings,
    })
}

fn kind_label(kind: ImageFilesystemKind) -> String {
    match kind {
        ImageFilesystemKind::Ntfs => "NTFS".to_string(),
        ImageFilesystemKind::Fat => "FAT".to_string(),
        ImageFilesystemKind::BitLocker => "BitLocker".to_string(),
    }
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
    Ok(detect_boot_filesystem(&sector))
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
