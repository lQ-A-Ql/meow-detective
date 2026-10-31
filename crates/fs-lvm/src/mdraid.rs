//! Read-only Linux MD RAID v1 metadata and RAID0 logical reads.

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use evidence_core::{EvidenceReader, ReaderInfo};

const MD_SB_MAGIC: u32 = 0xa92b4efc;
const MD_SB_V1_MIN_BYTES: usize = 256;
const SECTOR_SIZE: u64 = 512;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdRaidSuperblock {
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
    let chunk_sectors = read_u32(bytes, 88)?;
    if chunk_sectors == 0 {
        return Err(MdRaidError::InvalidChunkSize);
    }
    Ok(MdRaidSuperblock {
        major_version,
        feature_map: read_u32(bytes, 8)?,
        array_uuid: read_array(bytes, 16)?,
        name: read_name(bytes, 32)?,
        level,
        layout: read_u32(bytes, 76)?,
        size_sectors: read_u64(bytes, 80)?,
        chunk_sectors,
        raid_disks: read_u32(bytes, 92)?,
        data_offset_sectors: read_u64(bytes, 128)?,
        data_size_sectors: read_u64(bytes, 136)?,
        super_offset_sectors: read_u64(bytes, 144)?,
        dev_number: read_u32(bytes, 160)?,
        device_uuid: read_array(bytes, 168)?,
    })
}

pub fn read_superblock<R: Read + Seek + ?Sized>(
    reader: &mut R,
    offset: u64,
) -> Result<MdRaidSuperblock, MdRaidError> {
    let absolute = offset.checked_add(0).ok_or(MdRaidError::SizeOverflow)?;
    reader.seek(SeekFrom::Start(absolute))?;
    let mut bytes = [0u8; MD_SB_V1_MIN_BYTES];
    reader.read_exact(&mut bytes)?;
    parse_superblock(&bytes)
}

pub struct MdRaid0Reader {
    members: Vec<MdRaidMember>,
    by_number: Vec<usize>,
    info: ReaderInfo,
    chunk_bytes: u64,
    logical_size: u64,
    position: u64,
}

impl MdRaid0Reader {
    pub fn open(members: Vec<MdRaidMember>) -> Result<Self, MdRaidError> {
        let first = members.first().ok_or(MdRaidError::EmptyMembers)?;
        if first.superblock.raid_disks == 0 {
            return Err(MdRaidError::EmptyMembers);
        }
        if members.len() != first.superblock.raid_disks as usize {
            return Err(MdRaidError::MemberCount {
                expected: first.superblock.raid_disks,
                actual: members.len(),
            });
        }
        let chunk_bytes = first.superblock.chunk_bytes()?;
        let mut by_number = vec![usize::MAX; members.len()];
        let mut member_data_size = u64::MAX;
        for (index, member) in members.iter().enumerate() {
            let sb = &member.superblock;
            if !sb.is_raid0() {
                return Err(MdRaidError::UnsupportedLevel(sb.level));
            }
            if sb.array_uuid != first.superblock.array_uuid
                || sb.raid_disks != first.superblock.raid_disks
                || sb.chunk_sectors != first.superblock.chunk_sectors
                || sb.layout != first.superblock.layout
            {
                return Err(MdRaidError::MetadataConflict(
                    "array UUID, member count, chunk, or layout differs".to_string(),
                ));
            }
            let number =
                usize::try_from(sb.dev_number).map_err(|_| MdRaidError::NonContiguousMembers)?;
            if number >= by_number.len() || by_number[number] != usize::MAX {
                return Err(MdRaidError::NonContiguousMembers);
            }
            by_number[number] = index;
            member_data_size = member_data_size.min(sb.data_size_sectors);
            let data_end = sb
                .data_offset_sectors
                .checked_add(sb.data_size_sectors)
                .ok_or(MdRaidError::InvalidDataRange)?
                .checked_mul(SECTOR_SIZE)
                .ok_or(MdRaidError::InvalidDataRange)?;
            if data_end > member.reader.info().size {
                return Err(MdRaidError::InvalidDataRange);
            }
        }
        if by_number.contains(&usize::MAX) || member_data_size == 0 {
            return Err(MdRaidError::NonContiguousMembers);
        }
        let logical_size = member_data_size
            .checked_mul(u64::from(first.superblock.raid_disks))
            .and_then(|sectors| sectors.checked_mul(SECTOR_SIZE))
            .ok_or(MdRaidError::SizeOverflow)?;
        let array_id = format_hex(&first.superblock.array_uuid);
        Ok(Self {
            members,
            by_number,
            info: ReaderInfo {
                path: PathBuf::from(format!("mdraid0://{array_id}")),
                size: logical_size,
                kind: "Linux MD RAID0".to_string(),
            },
            chunk_bytes,
            logical_size,
            position: 0,
        })
    }

    pub fn logical_size(&self) -> u64 {
        self.logical_size
    }

    fn read_at_inner(&mut self, mut logical: u64, mut output: &mut [u8]) -> std::io::Result<usize> {
        let mut total = 0usize;
        while !output.is_empty() && logical < self.logical_size {
            let stripe = logical / self.chunk_bytes;
            let within = logical % self.chunk_bytes;
            let member_number = (stripe % self.by_number.len() as u64) as usize;
            let member_index = self.by_number[member_number];
            let member = &mut self.members[member_index];
            let member_chunk = stripe / self.by_number.len() as u64;
            let member_offset = member
                .superblock
                .data_offset_sectors
                .checked_mul(SECTOR_SIZE)
                .and_then(|base| base.checked_add(member_chunk.checked_mul(self.chunk_bytes)?))
                .and_then(|base| base.checked_add(within))
                .ok_or_else(|| std::io::Error::other("MD RAID offset overflow"))?;
            let available = (self.chunk_bytes - within)
                .min(self.logical_size - logical)
                .min(output.len() as u64) as usize;
            member.reader.seek(SeekFrom::Start(member_offset))?;
            member.reader.read_exact(&mut output[..available])?;
            total += available;
            logical += available as u64;
            output = &mut output[available..];
        }
        Ok(total)
    }
}

impl Read for MdRaid0Reader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.read_at_inner(self.position, buffer)?;
        self.position = self.position.saturating_add(read as u64);
        Ok(read)
    }
}

impl Seek for MdRaid0Reader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            SeekFrom::Start(value) => value,
            SeekFrom::End(value) => signed_seek(self.logical_size, value)?,
            SeekFrom::Current(value) => signed_seek(self.position, value)?,
        };
        self.position = next;
        Ok(next)
    }
}

impl EvidenceReader for MdRaid0Reader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

fn signed_seek(base: u64, offset: i64) -> std::io::Result<u64> {
    if offset >= 0 {
        base.checked_add(offset as u64)
    } else {
        base.checked_sub(offset.unsigned_abs())
    }
    .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek outside MD RAID"))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, MdRaidError> {
    bytes
        .get(offset..offset + 4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
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

#[cfg(test)]
#[path = "../tests/unit/mdraid.rs"]
mod tests;
