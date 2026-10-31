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
    zones: Vec<Raid0Zone>,
    info: ReaderInfo,
    chunk_bytes: u64,
    layout: u32,
    logical_size: u64,
    position: u64,
}

struct Raid0Zone {
    logical_start: u64,
    logical_end: u64,
    device_start: u64,
    members: Vec<usize>,
}

impl MdRaid0Reader {
    pub fn open(members: Vec<MdRaidMember>) -> Result<Self, MdRaidError> {
        let first = members.first().ok_or(MdRaidError::EmptyMembers)?;
        let first_layout = first.superblock.layout;
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
        let mut member_sizes = vec![0u64; members.len()];
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
            member_sizes[number] =
                sb.data_size_sectors / u64::from(sb.chunk_sectors) * u64::from(sb.chunk_sectors);
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
        if by_number.contains(&usize::MAX) || member_sizes.contains(&0) {
            return Err(MdRaidError::NonContiguousMembers);
        }
        let zones = build_zones(&member_sizes, chunk_bytes, &by_number)?;
        let logical_size = zones
            .last()
            .map(|zone| zone.logical_end)
            .ok_or(MdRaidError::EmptyMembers)?;
        let array_id = format_hex(&first.superblock.array_uuid);
        Ok(Self {
            members,
            zones,
            info: ReaderInfo {
                path: PathBuf::from(format!("mdraid0://{array_id}")),
                size: logical_size,
                kind: "Linux MD RAID0".to_string(),
            },
            chunk_bytes,
            layout: first_layout,
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
            let zone = self
                .zones
                .iter()
                .find(|zone| logical < zone.logical_end)
                .ok_or_else(|| std::io::Error::other("MD RAID logical offset is outside zones"))?;
            let zone_logical = logical - zone.logical_start;
            let stripe = zone_logical / self.chunk_bytes;
            let within = zone_logical % self.chunk_bytes;
            let global_stripe = logical / self.chunk_bytes;
            let member_number = if self.layout_is_original() {
                (global_stripe as usize) % zone.members.len()
            } else {
                (stripe as usize) % zone.members.len()
            };
            let member_index = zone.members[member_number];
            let member = &mut self.members[member_index];
            let member_chunk = stripe / zone.members.len() as u64;
            let member_offset = member
                .superblock
                .data_offset_sectors
                .checked_mul(SECTOR_SIZE)
                .and_then(|base| base.checked_add(zone.device_start))
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

    fn layout_is_original(&self) -> bool {
        self.layout == 1
    }
}

fn build_zones(
    member_sizes: &[u64],
    chunk_bytes: u64,
    by_number: &[usize],
) -> Result<Vec<Raid0Zone>, MdRaidError> {
    let mut thresholds = member_sizes.to_vec();
    thresholds.sort_unstable();
    thresholds.dedup();
    let mut zones = Vec::new();
    let mut device_start_sectors = 0u64;
    let mut logical_start = 0u64;
    let chunk_sectors = chunk_bytes / SECTOR_SIZE;
    for threshold in thresholds {
        let members = member_sizes
            .iter()
            .enumerate()
            .filter_map(|(number, size)| (*size >= threshold).then_some(by_number[number]))
            .collect::<Vec<_>>();
        if members.is_empty() || threshold <= device_start_sectors || chunk_sectors == 0 {
            continue;
        }
        let zone_sectors = threshold
            .checked_sub(device_start_sectors)
            .and_then(|length| length.checked_mul(members.len() as u64))
            .ok_or(MdRaidError::SizeOverflow)?;
        let logical_end = logical_start
            .checked_add(
                zone_sectors
                    .checked_mul(SECTOR_SIZE)
                    .ok_or(MdRaidError::SizeOverflow)?,
            )
            .ok_or(MdRaidError::SizeOverflow)?;
        zones.push(Raid0Zone {
            logical_start,
            logical_end,
            device_start: device_start_sectors * SECTOR_SIZE,
            members,
        });
        logical_start = logical_end;
        device_start_sectors = threshold;
    }
    Ok(zones)
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
