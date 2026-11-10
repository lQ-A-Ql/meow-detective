use super::{MdRaidError, MdRaidMetadataVersion, MdRaidSuperblock, SECTOR_SIZE};
use std::io::{Read, Seek, SeekFrom};

const SUPERBLOCK_BYTES: usize = 4096;
const WORDS: usize = SUPERBLOCK_BYTES / 4;
const CHECKSUM_WORD: usize = 38;
const DISKS_OFFSET: usize = 128;
const DISK_WORDS: usize = 32;
const DISK_COUNT: usize = 27;
const THIS_DISK_OFFSET: usize = 992;
const ROLE_SPARE: u16 = 0xffff;
const ROLE_FAULTY: u16 = 0xfffe;
const ROLE_JOURNAL: u16 = 0xfffd;

pub(super) fn read_superblock<R: Read + Seek + ?Sized>(
    reader: &mut R,
    byte_offset: u64,
    super_sector: u64,
) -> Result<MdRaidSuperblock, MdRaidError> {
    reader.seek(SeekFrom::Start(byte_offset))?;
    let mut bytes = [0u8; SUPERBLOCK_BYTES];
    reader.read_exact(&mut bytes)?;
    parse(&bytes, super_sector)
}

fn parse(bytes: &[u8], super_sector: u64) -> Result<MdRaidSuperblock, MdRaidError> {
    let magic_le = u32::from_le_bytes(read_word(bytes, 0)?);
    let magic_be = u32::from_be_bytes(read_word(bytes, 0)?);
    let big_endian = if magic_le == super::MD_SB_MAGIC {
        false
    } else if magic_be == super::MD_SB_MAGIC {
        true
    } else {
        return Err(MdRaidError::InvalidMagic);
    };
    let word = |index: usize| -> Result<u32, MdRaidError> {
        let raw = read_word(bytes, index)?;
        Ok(if big_endian {
            u32::from_be_bytes(raw)
        } else {
            u32::from_le_bytes(raw)
        })
    };
    if word(1)? != 0 || word(2)? != 90 {
        return Err(MdRaidError::UnsupportedVersion(0));
    }
    let checksum = checksum(bytes, big_endian)?;
    let stored_checksum = word(CHECKSUM_WORD)?;
    let raid_disks = word(10)?;
    let dev_number = word(THIS_DISK_OFFSET)?;
    let mut dev_roles = vec![ROLE_SPARE; DISK_COUNT];
    for disk in 0..DISK_COUNT {
        let base = DISKS_OFFSET + disk * DISK_WORDS;
        let number = word(base)? as usize;
        if number >= dev_roles.len() {
            dev_roles.resize(number + 1, ROLE_SPARE);
        }
        let role = word(base + 3)? as u16;
        if !matches!(role, ROLE_SPARE | ROLE_FAULTY | ROLE_JOURNAL) {
            dev_roles[number] = role;
        }
    }
    let chunk_bytes = word(65)?;
    if chunk_bytes % SECTOR_SIZE as u32 != 0 {
        return Err(MdRaidError::InvalidChunkSize);
    }
    let (events_low, events_high) = if big_endian {
        (word(8)?, word(7)?)
    } else {
        (word(7)?, word(8)?)
    };
    Ok(MdRaidSuperblock {
        metadata_version: MdRaidMetadataVersion::V0_90,
        major_version: 0,
        feature_map: 0,
        array_uuid: bytes[20..36]
            .try_into()
            .map_err(|_| MdRaidError::Truncated)?,
        name: String::new(),
        level: word(7)? as i32,
        layout: word(64)?,
        size_sectors: u64::from(word(8)?),
        chunk_sectors: chunk_bytes / SECTOR_SIZE as u32,
        raid_disks,
        data_offset_sectors: 0,
        data_size_sectors: u64::from(word(8)?),
        super_offset_sectors: super_sector,
        dev_number,
        device_uuid: [0; 16],
        events: (u64::from(events_high) << 32) | u64::from(events_low),
        recovery_offset_sectors: u64::from(word(43)?),
        resync_offset_sectors: u64::MAX,
        reshape_position_sectors: 0,
        dev_roles,
        checksum_valid: (stored_checksum & 0xffff) == checksum,
    })
}

fn checksum(bytes: &[u8], big_endian: bool) -> Result<u32, MdRaidError> {
    let mut sum = 0u64;
    for index in 0..WORDS {
        if index == CHECKSUM_WORD {
            continue;
        }
        let raw = read_word(bytes, index)?;
        sum += u64::from(if big_endian {
            u32::from_be_bytes(raw)
        } else {
            u32::from_le_bytes(raw)
        });
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    Ok(sum as u32)
}

fn read_word(bytes: &[u8], index: usize) -> Result<[u8; 4], MdRaidError> {
    let offset = index.checked_mul(4).ok_or(MdRaidError::SizeOverflow)?;
    bytes
        .get(offset..offset + 4)
        .and_then(|word| word.try_into().ok())
        .ok_or(MdRaidError::Truncated)
}

#[cfg(test)]
#[path = "../../tests/unit/mdraid_v090.rs"]
mod tests;
