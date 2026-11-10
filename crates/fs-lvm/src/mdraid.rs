//! Read-only Linux MD RAID metadata and logical-volume reads.

use std::io::{Read, Seek, SeekFrom};

use evidence_core::EvidenceReader;

mod reader;
mod v090;

pub use reader::{MdRaidReader, RaidMemberGeometry};

const MD_SB_MAGIC: u32 = 0xa92b4efc;
const MD_SB_V1_MIN_BYTES: usize = 256;
const SECTOR_SIZE: u64 = 512;
const ROLE_MAX: u16 = 0xff00;

#[derive(Debug, thiserror::Error)]
pub enum MdRaidError {
    #[error("MD superblock is truncated")]
    Truncated,
    #[error("MD superblock magic is invalid")]
    InvalidMagic,
    #[error("MD superblock version {0} is unsupported")]
    UnsupportedVersion(u32),
    #[error("MD RAID level {0} is unsupported")]
    UnsupportedLevel(i32),
    #[error("MD RAID requires at least one member")]
    EmptyMembers,
    #[error("MD RAID member count is inconsistent: expected {expected}, found {actual}")]
    MemberCount { expected: u32, actual: usize },
    #[error("MD RAID member numbers are not contiguous")]
    NonContiguousMembers,
    #[error("MD RAID metadata conflicts between members: {0}")]
    MetadataConflict(String),
    #[error("MD RAID chunk size is invalid")]
    InvalidChunkSize,
    #[error("MD RAID member data range is invalid")]
    InvalidDataRange,
    #[error("MD RAID logical size overflows")]
    SizeOverflow,
    #[error("MD RAID member index {0} is unavailable")]
    MissingMember(u32),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MdRaidSuperblock {
    pub metadata_version: MdRaidMetadataVersion,
    pub major_version: u32,
    pub feature_map: u32,
    pub array_uuid: [u8; 16],
    pub name: String,
    pub level: i32,
    pub layout: u32,
    pub size_sectors: u64,
    pub chunk_sectors: u32,
    pub raid_disks: u32,
    pub data_offset_sectors: u64,
    pub data_size_sectors: u64,
    pub super_offset_sectors: u64,
    pub dev_number: u32,
    pub device_uuid: [u8; 16],
    pub events: u64,
    pub recovery_offset_sectors: u64,
    pub resync_offset_sectors: u64,
    pub reshape_position_sectors: u64,
    pub dev_roles: Vec<u16>,
    pub checksum_valid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MdRaidMetadataVersion {
    V0_90,
    V1_0,
    V1_1,
    V1_2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdRaidSuperblockProbe {
    pub superblock: MdRaidSuperblock,
    pub byte_offset: u64,
}

impl MdRaidSuperblock {
    pub fn is_raid0(&self) -> bool {
        self.level == 0
    }

    pub fn chunk_bytes(&self) -> Result<u64, MdRaidError> {
        u64::from(self.chunk_sectors)
            .checked_mul(SECTOR_SIZE)
            .filter(|value| *value != 0)
            .ok_or(MdRaidError::InvalidChunkSize)
    }

    pub fn slot_index(&self) -> Option<u32> {
        let role = *self.dev_roles.get(self.dev_number as usize)?;
        (role < ROLE_MAX && u32::from(role) < self.raid_disks).then_some(u32::from(role))
    }

    pub fn is_active_slot(&self) -> bool {
        self.slot_index().is_some()
    }

    pub fn is_rebuilding(&self) -> bool {
        self.recovery_offset_sectors != u64::MAX
    }
}

pub struct MdRaidMember {
    pub superblock: MdRaidSuperblock,
    pub reader: Box<dyn EvidenceReader>,
}

impl MdRaidMember {
    pub fn new(superblock: MdRaidSuperblock, reader: Box<dyn EvidenceReader>) -> Self {
        Self { superblock, reader }
    }
}

pub fn parse_superblock(bytes: &[u8]) -> Result<MdRaidSuperblock, MdRaidError> {
    parse_superblock_version(bytes, MdRaidMetadataVersion::V1_2)
}

pub fn parse_superblock_version(
    bytes: &[u8],
    metadata_version: MdRaidMetadataVersion,
) -> Result<MdRaidSuperblock, MdRaidError> {
    if bytes.len() < MD_SB_V1_MIN_BYTES {
        return Err(MdRaidError::Truncated);
    }
    let magic = read_u32(bytes, 0)?;
    if magic != MD_SB_MAGIC {
        return Err(MdRaidError::InvalidMagic);
    }
    let major_version = read_u32(bytes, 4)?;
    if major_version != 1 {
        return Err(MdRaidError::UnsupportedVersion(major_version));
    }
    let level = read_u32(bytes, 72)? as i32;
    if level != 0 {
        return Err(MdRaidError::UnsupportedLevel(level));
    }
    let max_dev = read_u32(bytes, 220)? as usize;
    let role_end = MD_SB_V1_MIN_BYTES
        .checked_add(max_dev.checked_mul(2).ok_or(MdRaidError::SizeOverflow)?)
        .ok_or(MdRaidError::SizeOverflow)?;
    if bytes.len() < role_end {
        return Err(MdRaidError::Truncated);
    }
    let stored_checksum = read_u32(bytes, 216)?;
    let computed_checksum = v1_checksum(&bytes[..role_end]);
    let mut dev_roles = Vec::with_capacity(max_dev);
    for index in 0..max_dev {
        dev_roles.push(read_u16(bytes, MD_SB_V1_MIN_BYTES + index * 2)?);
    }
    Ok(MdRaidSuperblock {
        metadata_version,
        major_version,
        feature_map: read_u32(bytes, 8)?,
        array_uuid: read_array(bytes, 16)?,
        name: read_name(bytes, 32)?,
        level,
        layout: read_u32(bytes, 76)?,
        size_sectors: read_u64(bytes, 80)?,
        chunk_sectors: read_u32(bytes, 88)?,
        raid_disks: read_u32(bytes, 92)?,
        data_offset_sectors: read_u64(bytes, 128)?,
        data_size_sectors: read_u64(bytes, 136)?,
        super_offset_sectors: read_u64(bytes, 144)?,
        dev_number: read_u32(bytes, 160)?,
        device_uuid: read_array(bytes, 168)?,
        events: read_u64(bytes, 200)?,
        recovery_offset_sectors: read_u64(bytes, 152)?,
        resync_offset_sectors: read_u64(bytes, 208)?,
        reshape_position_sectors: read_u64(bytes, 104)?,
        dev_roles,
        checksum_valid: stored_checksum == computed_checksum,
    })
}

pub fn read_superblock<R: Read + Seek + ?Sized>(
    reader: &mut R,
    offset: u64,
) -> Result<MdRaidSuperblock, MdRaidError> {
    read_superblock_version(reader, offset, MdRaidMetadataVersion::V1_2)
}

pub fn read_superblock_version<R: Read + Seek + ?Sized>(
    reader: &mut R,
    offset: u64,
    metadata_version: MdRaidMetadataVersion,
) -> Result<MdRaidSuperblock, MdRaidError> {
    reader.seek(SeekFrom::Start(offset))?;
    let mut header = [0u8; MD_SB_V1_MIN_BYTES];
    reader.read_exact(&mut header)?;
    let max_dev = read_u32(&header, 220)? as usize;
    let role_bytes = max_dev.checked_mul(2).ok_or(MdRaidError::SizeOverflow)?;
    let mut bytes = vec![0u8; MD_SB_V1_MIN_BYTES + role_bytes];
    bytes[..MD_SB_V1_MIN_BYTES].copy_from_slice(&header);
    if role_bytes > 0 {
        reader.read_exact(&mut bytes[MD_SB_V1_MIN_BYTES..])?;
    }
    parse_superblock_version(&bytes, metadata_version)
}

pub fn probe_superblock<R: Read + Seek + ?Sized>(
    reader: &mut R,
    device_bytes: u64,
) -> Option<MdRaidSuperblockProbe> {
    let sectors = device_bytes / SECTOR_SIZE;
    let tail_sector = sectors.saturating_sub(16) & !7;
    let versioned_offsets = [
        (4096u64, 8u64, MdRaidMetadataVersion::V1_2),
        (0u64, 0u64, MdRaidMetadataVersion::V1_1),
        (
            tail_sector.saturating_mul(SECTOR_SIZE),
            tail_sector,
            MdRaidMetadataVersion::V1_0,
        ),
    ];
    for (byte_offset, sector, version) in versioned_offsets {
        if byte_offset.checked_add(MD_SB_V1_MIN_BYTES as u64)? > device_bytes {
            continue;
        }
        let Ok(superblock) = read_superblock_version(reader, byte_offset, version) else {
            continue;
        };
        if superblock.checksum_valid
            && superblock.super_offset_sectors == sector
            && is_supported_level(superblock.level)
        {
            return Some(MdRaidSuperblockProbe {
                superblock,
                byte_offset,
            });
        }
    }
    let v090_sector = sectors.checked_sub(128).map(|value| value & !127)?;
    let v090_offset = v090_sector.checked_mul(SECTOR_SIZE)?;
    if v090_offset.checked_add(4096)? <= device_bytes {
        if let Ok(superblock) = v090::read_superblock(reader, v090_offset, v090_sector) {
            if superblock.checksum_valid && is_supported_level(superblock.level) {
                return Some(MdRaidSuperblockProbe {
                    superblock,
                    byte_offset: v090_offset,
                });
            }
        }
    }
    None
}

fn is_supported_level(level: i32) -> bool {
    matches!(level, -1 | 0 | 1 | 4 | 5 | 6 | 10)
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, MdRaidError> {
    bytes
        .get(offset..offset + 4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(MdRaidError::Truncated)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, MdRaidError> {
    bytes
        .get(offset..offset + 2)
        .and_then(|value| value.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or(MdRaidError::Truncated)
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, MdRaidError> {
    bytes
        .get(offset..offset + 8)
        .and_then(|value| value.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or(MdRaidError::Truncated)
}

fn read_array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], MdRaidError> {
    bytes
        .get(offset..offset + N)
        .and_then(|value| value.try_into().ok())
        .ok_or(MdRaidError::Truncated)
}

fn read_name(bytes: &[u8], offset: usize) -> Result<String, MdRaidError> {
    let raw = bytes
        .get(offset..offset + 32)
        .ok_or(MdRaidError::Truncated)?;
    let length = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
    Ok(String::from_utf8_lossy(&raw[..length]).to_string())
}

fn format_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

pub fn v1_checksum(bytes: &[u8]) -> u32 {
    let (words, remainder) = bytes.as_chunks::<4>();
    let mut sum = 0u64;
    for (index, word) in words.iter().enumerate() {
        if index != 54 {
            sum += u64::from(u32::from_le_bytes(*word));
        }
    }
    if let [low, high] = remainder {
        sum += u64::from(u16::from_le_bytes([*low, *high]));
    }
    ((sum & 0xffff_ffff) + (sum >> 32)) as u32
}

pub(super) fn signed_seek(base: u64, offset: i64) -> std::io::Result<u64> {
    if offset >= 0 {
        base.checked_add(offset as u64)
    } else {
        base.checked_sub(offset.unsigned_abs())
    }
    .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek outside MD RAID"))
}

#[cfg(test)]
#[path = "../tests/unit/mdraid.rs"]
mod tests;
